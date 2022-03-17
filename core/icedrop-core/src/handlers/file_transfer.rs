use super::handshake::HandshakeResponseFrame;
use super::session::EndSessionFrame;
use super::utils::def_frame_selector;
use crate::endpoint::EndpointHandle;
use crate::proto::{Frame, FrameHandler, FrameParsingResult};

use std::path::Path;
use std::time;

use async_trait::async_trait;
use byteorder::{ByteOrder, LittleEndian};
use tokio::{
    fs::File,
    io::{AsyncReadExt, AsyncWriteExt},
};

#[derive(Debug)]
pub struct FileTransferAckFrame {
    segment_idx: u32,
}

#[async_trait]
impl Frame for FileTransferAckFrame {
    fn frame_type(&self) -> u16 {
        return 4;
    }

    fn try_parse(frame_type: u16, buf: Vec<u8>) -> FrameParsingResult<Self> {
        if frame_type != 4 {
            return FrameParsingResult::Skip(buf);
        }

        let segment_idx = LittleEndian::read_u32(&buf);

        FrameParsingResult::Ok(FileTransferAckFrame { segment_idx })
    }

    fn to_bytes(self) -> Vec<u8> {
        let mut segment_idx_buf = [0 as u8; 4];
        LittleEndian::write_u32(&mut segment_idx_buf, self.segment_idx);

        let mut buf = Vec::<u8>::with_capacity(4);
        buf.extend(segment_idx_buf);

        buf
    }
}

def_frame_selector!(
    FileTransferNextFrame,
    HandshakeResponseFrame,
    FileTransferAckFrame
);

def_frame_selector!(
    FileTransferAckOrEndFrame,
    FileTransferAckFrame,
    EndSessionFrame
);

#[derive(Debug)]
pub struct FileTransferDataFrame {
    segment_idx: u32,
    chunk_size: u32,
    data: Vec<u8>,
}

#[async_trait]
impl Frame for FileTransferDataFrame {
    fn frame_type(&self) -> u16 {
        return 3;
    }

    fn try_parse(frame_type: u16, mut buf: Vec<u8>) -> FrameParsingResult<Self> {
        if frame_type != 3 {
            return FrameParsingResult::Skip(buf);
        }

        let segment_idx = LittleEndian::read_u32(&buf[0..4]);
        let chunk_size = LittleEndian::read_u32(&buf[4..8]) as usize;

        let mut data = buf.split_off(8);
        data.resize(chunk_size, 0);

        FrameParsingResult::Ok(FileTransferDataFrame {
            segment_idx,
            chunk_size: chunk_size as u32,
            data,
        })
    }

    fn to_bytes(self) -> Vec<u8> {
        let mut segment_idx_buf = [0 as u8; 4];
        LittleEndian::write_u32(&mut segment_idx_buf, self.segment_idx);

        let mut chunk_size_buf = [0 as u8; 4];
        LittleEndian::write_u32(&mut chunk_size_buf, self.chunk_size);

        let mut buf = Vec::<u8>::with_capacity(8 + self.data.len());
        buf.extend(segment_idx_buf);
        buf.extend(chunk_size_buf);
        buf.extend(self.data);

        buf
    }
}

pub enum FileTransferEvent {
    SegmentSent(u32, usize),
    Complete,
}

pub struct FileTransferNextHandler {
    endpoint_handle: EndpointHandle,
    file: Option<File>,
    cur_segment: u32,
    callback_fn: Option<Box<dyn Fn(FileTransferEvent) + Send>>,
}

impl FileTransferNextHandler {
    pub fn new(endpoint_handle: EndpointHandle, file: File) -> Self {
        Self {
            endpoint_handle,
            file: Some(file),
            cur_segment: 0,
            callback_fn: None,
        }
    }

    pub fn set_callback_fn<F>(&mut self, f: F)
    where
        F: Fn(FileTransferEvent) + Send + 'static,
    {
        self.callback_fn = Some(Box::new(f));
    }

    async fn send_segment(file: &mut File, segment_idx: u32, handle: &EndpointHandle) -> usize {
        // Read the file as much as possible (within the chunk size limit).
        let chunk_size = 1024 * 512;
        let mut total_read_size = 0 as usize;
        let mut buf = Vec::<u8>::with_capacity(chunk_size);
        unsafe {
            buf.set_len(chunk_size);
        }
        while total_read_size < chunk_size {
            let read_size = file.read(&mut buf[total_read_size..]).await.unwrap();
            if read_size == 0 {
                // Eof encountered, stop reading.
                break;
            }
            total_read_size += read_size;
        }

        // Resize the buffer to the final read size.
        buf.resize(total_read_size, 0);

        handle
            .send_frame(FileTransferDataFrame {
                segment_idx,
                chunk_size: total_read_size as u32,
                data: buf,
            })
            .await
            .unwrap();

        total_read_size
    }
}

#[async_trait]
impl FrameHandler for FileTransferNextHandler {
    type IncomingFrame = FileTransferNextFrame;

    async fn handle_frame(&mut self, frame: Self::IncomingFrame) {
        if let FileTransferNextFrame::FileTransferAckFrame(frame) = frame {
            if frame.segment_idx < self.cur_segment {
                panic!("Unexpected next segment.");
            }

            self.cur_segment = frame.segment_idx;

            // Invoke event callback if necessary.
            if let Some(fn_box) = &mut self.callback_fn {
                fn_box.call((FileTransferEvent::SegmentSent(
                    self.cur_segment,
                    self.cur_segment as usize * 1024 * 512,
                ),));
            }
        } else {
            let mut file = self.file.take().unwrap();
            let handle = self.endpoint_handle.clone();

            // Start sending "thread".
            let rt = tokio::runtime::Handle::current();
            rt.spawn(async move {
                let mut segment_id = 0;
                loop {
                    let bytes_sent = Self::send_segment(&mut file, segment_id, &handle).await;
                    segment_id += 1;

                    // Invoke event callback with complete event when there is no more data to send.
                    if bytes_sent == 0 {
                        break;
                    }
                }
            });
        };
    }
}

pub struct FileTransferReceivingHandler {
    endpoint_handle: EndpointHandle,
    file: File,
    windowed_recv_segments: u32,
    #[cfg(debug_assertions)]
    last_recv_timestamp: Option<time::Instant>,
}

impl FileTransferReceivingHandler {
    pub async fn new<P>(endpoint_handle: EndpointHandle, path: P) -> Self
    where
        P: AsRef<Path>,
    {
        let file_path = path.as_ref().join("test");
        let file = File::create(file_path).await.unwrap();
        Self {
            endpoint_handle,
            file,
            windowed_recv_segments: 0,
            #[cfg(debug_assertions)]
            last_recv_timestamp: None,
        }
    }
}

#[async_trait]
impl FrameHandler for FileTransferReceivingHandler {
    type IncomingFrame = FileTransferDataFrame;

    async fn handle_frame(&mut self, frame: Self::IncomingFrame) {
        #[cfg(debug_assertions)]
        {
            let now = time::Instant::now();
            let speed = if let Some(ts) = self.last_recv_timestamp {
                (frame.chunk_size as f64 / 1048576_f64) / (now - ts).as_secs_f64()
            } else {
                0_f64
            };
            self.last_recv_timestamp = Some(now);
            println!(
                "receive data frame: {} ({} bytes, {:.2} MB/s)",
                frame.segment_idx, frame.chunk_size, speed
            );
        }

        if frame.chunk_size == 0 {
            self.file.flush().await.unwrap();
            self.endpoint_handle
                .send_frame(FileTransferAckOrEndFrame::EndSessionFrame(EndSessionFrame))
                .await
                .unwrap();
            return;
        }

        self.file.write_all(&frame.data).await.unwrap();

        self.windowed_recv_segments += 1;
        if self.windowed_recv_segments >= 8 {
            self.windowed_recv_segments = 0;
            self.endpoint_handle
                .send_frame(FileTransferAckOrEndFrame::FileTransferAckFrame(
                    FileTransferAckFrame {
                        segment_idx: frame.segment_idx + 1,
                    },
                ))
                .await
                .unwrap();
        }
    }
}
