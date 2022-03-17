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
    file: File,
    sending_window: u32,
    cur_segment: u32,
    bytes_sent: usize,
    finished: bool,
    callback_fn: Option<Box<dyn Fn(FileTransferEvent) + Send>>,
}

impl FileTransferNextHandler {
    pub fn new(endpoint_handle: EndpointHandle, file: File) -> Self {
        Self {
            endpoint_handle,
            file,
            sending_window: 0,
            cur_segment: 1,
            bytes_sent: 0,
            finished: false,
            callback_fn: None,
        }
    }

    pub fn set_callback_fn<F>(&mut self, f: F)
    where
        F: Fn(FileTransferEvent) + Send + 'static,
    {
        self.callback_fn = Some(Box::new(f));
    }

    async fn send_segment(&mut self) -> usize {
        // Read the file as much as possible (within the chunk size limit).
        let chunk_size = 1024 * 512;
        let mut total_read_size = 0 as usize;
        let mut buf = Vec::<u8>::with_capacity(chunk_size);
        buf.resize(chunk_size, 0);
        while total_read_size < chunk_size {
            let read_size = self.file.read(&mut buf[total_read_size..]).await.unwrap();
            if read_size == 0 {
                // Eof encountered, stop reading.
                break;
            }
            total_read_size += read_size;
        }

        // Resize the buffer to the final read size.
        buf.resize(total_read_size, 0);

        let segment_idx = self.cur_segment;
        self.cur_segment += 1;

        self.bytes_sent += total_read_size;

        self.endpoint_handle
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
            if frame.segment_idx > self.cur_segment {
                panic!("Unexpected next segment.");
            }

            // Invoke event callback if necessary.
            if let Some(fn_box) = &mut self.callback_fn {
                fn_box.call((FileTransferEvent::SegmentSent(
                    self.cur_segment - 1,
                    self.bytes_sent,
                ),));
            }

            // Increase the sending window when a segment is notified to be received.
            self.sending_window = (self.sending_window + 8).min(64);
        } else {
            // Set the initial sending window.
            self.sending_window = 8;
        };

        while self.sending_window > 0 && !self.finished {
            self.sending_window -= 1;
            let bytes_sent = self.send_segment().await;

            // Invoke event callback with complete event when there is no more data to send.
            if bytes_sent == 0 {
                if let Some(fn_box) = &mut self.callback_fn {
                    fn_box.call((FileTransferEvent::Complete,));
                }
                self.finished = true;
                break;
            }
        }
    }
}

pub struct FileTransferReceivingHandler {
    endpoint_handle: EndpointHandle,
    file: File,
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
