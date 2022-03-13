use super::handshake::HandshakeResponseFrame;
use super::session::EndSessionFrame;
use super::utils::def_frame_selector;
use crate::proto::{Frame, FrameHandler, Stream};

use std::{error::Error, path::Path};

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

    async fn parse<S>(
        frame_type: u16,
        stream: &mut S,
    ) -> Option<Result<Self, Box<dyn Error + Send>>>
    where
        S: Stream,
    {
        if frame_type != 4 {
            return None;
        }

        let mut segment_idx_buf = [0 as u8; 4];
        let _ = stream.read_exact(&mut segment_idx_buf).await;

        let segment_idx = LittleEndian::read_u32(&segment_idx_buf);

        Some(Ok(FileTransferAckFrame { segment_idx }))
    }

    fn to_bytes(&self) -> Vec<u8> {
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

    async fn parse<S>(
        frame_type: u16,
        stream: &mut S,
    ) -> Option<Result<Self, Box<dyn Error + Send>>>
    where
        S: Stream,
    {
        if frame_type != 3 {
            return None;
        }

        let mut segment_idx_buf = [0 as u8; 4];
        let _ = stream.read_exact(&mut segment_idx_buf).await;

        let mut chunk_size_buf = [0 as u8; 4];
        let _ = stream.read_exact(&mut chunk_size_buf).await;

        let segment_idx = LittleEndian::read_u32(&segment_idx_buf);
        let chunk_size = LittleEndian::read_u32(&chunk_size_buf) as usize;

        let mut data = Vec::<u8>::with_capacity(chunk_size);
        data.resize(chunk_size, 0);
        let read_size = stream.read_exact(&mut data).await;
        if read_size.unwrap() != chunk_size {
            panic!("Unexpected eof.");
        }

        Some(Ok(FileTransferDataFrame {
            segment_idx,
            chunk_size: chunk_size as u32,
            data,
        }))
    }

    fn to_bytes(&self) -> Vec<u8> {
        let mut segment_idx_buf = [0 as u8; 4];
        LittleEndian::write_u32(&mut segment_idx_buf, self.segment_idx);

        let mut chunk_size_buf = [0 as u8; 4];
        LittleEndian::write_u32(&mut chunk_size_buf, self.chunk_size);

        let mut buf = Vec::<u8>::with_capacity(4);
        buf.extend(segment_idx_buf);
        buf.extend(chunk_size_buf);
        buf.extend(&self.data);

        buf
    }
}

pub struct FileTransferNextHandler {
    file: File,
    cur_segment: u32,
}

impl FileTransferNextHandler {
    pub fn new(file: File) -> Self {
        Self {
            file,
            cur_segment: 1,
        }
    }
}

#[async_trait]
impl FrameHandler for FileTransferNextHandler {
    type IncomingFrame = FileTransferNextFrame;
    type OutgoingFrame = FileTransferDataFrame;

    async fn handle_frame(&mut self, frame: Self::IncomingFrame) -> Self::OutgoingFrame {
        if let FileTransferNextFrame::FileTransferAckFrame(frame) = frame {
            if frame.segment_idx != self.cur_segment {
                panic!("Unexpected next segment.");
            }
        }

        // Read the file as much as possible (within the chunk size limit).
        let chunk_size = 1024 * 1024 * 8;
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

        FileTransferDataFrame {
            segment_idx,
            chunk_size: total_read_size as u32,
            data: buf,
        }
    }
}

pub struct FileTransferReceivingHandler {
    file: File,
}

impl FileTransferReceivingHandler {
    pub async fn new<P>(path: P) -> Self
    where
        P: AsRef<Path>,
    {
        let file_path = path.as_ref().join("test");
        let file = File::create(file_path).await.unwrap();
        Self { file }
    }
}

#[async_trait]
impl FrameHandler for FileTransferReceivingHandler {
    type IncomingFrame = FileTransferDataFrame;
    type OutgoingFrame = FileTransferAckOrEndFrame;

    async fn handle_frame(&mut self, frame: Self::IncomingFrame) -> Self::OutgoingFrame {
        println!(
            "receive data frame: {} ({} bytes)",
            frame.segment_idx, frame.chunk_size
        );

        if frame.chunk_size == 0 {
            self.file.flush().await.unwrap();
            return FileTransferAckOrEndFrame::EndSessionFrame(EndSessionFrame);
        }

        self.file.write_all(&frame.data).await.unwrap();

        FileTransferAckOrEndFrame::FileTransferAckFrame(FileTransferAckFrame {
            segment_idx: frame.segment_idx + 1,
        })
    }
}
