use crate::proto::{Frame, FrameHandler, Stream};

use std::error::Error;

use async_trait::async_trait;
use byteorder::{ByteOrder, LittleEndian};

#[derive(Debug)]
pub struct HandshakeRequestFrame {
    pub name: String,
}

#[async_trait]
impl Frame for HandshakeRequestFrame {
    fn frame_type(&self) -> u16 {
        return 1;
    }

    async fn parse<S>(
        frame_type: u16,
        stream: &mut S,
    ) -> Option<Result<Self, Box<dyn Error + Send>>>
    where
        S: Stream,
    {
        if frame_type != 1 {
            return None;
        }

        let mut size_buf = [0 as u8; 4];
        let _ = stream.read_exact(&mut size_buf).await;

        let size = LittleEndian::read_u32(&size_buf) as usize;

        let mut name_buf = Vec::<u8>::with_capacity(size);
        name_buf.resize(size, 0);
        let _ = stream.read_exact(&mut name_buf).await;

        let name_result = String::from_utf8(name_buf);
        if let Ok(name) = name_result {
            let frame = HandshakeRequestFrame { name };
            Some(Ok(frame))
        } else {
            Some(Err(Box::new(name_result.unwrap_err())))
        }
    }

    fn to_bytes(&self) -> Vec<u8> {
        let mut size_buf = [0 as u8; 4];
        LittleEndian::write_u32(&mut size_buf, self.name.len() as u32);

        let mut buf = Vec::<u8>::with_capacity(4 + self.name.len());
        buf.extend(size_buf);
        buf.extend(self.name.as_bytes());

        buf
    }
}

#[derive(Debug)]
pub struct HandshakeResponseFrame;

#[async_trait]
impl Frame for HandshakeResponseFrame {
    fn frame_type(&self) -> u16 {
        return 2;
    }

    async fn parse<S>(
        frame_type: u16,
        _stream: &mut S,
    ) -> Option<Result<Self, Box<dyn Error + Send>>>
    where
        S: Stream,
    {
        if frame_type != 2 {
            return None;
        }

        Some(Ok(HandshakeResponseFrame))
    }

    fn to_bytes(&self) -> Vec<u8> {
        Vec::new()
    }
}

pub struct HandshakeHandler;

#[async_trait]
impl FrameHandler for HandshakeHandler {
    type IncomingFrame = HandshakeRequestFrame;
    type OutgoingFrame = HandshakeResponseFrame;

    async fn handle_frame(&mut self, frame: Self::IncomingFrame) -> Self::OutgoingFrame {
        println!("{}", frame.name);
        HandshakeResponseFrame
    }
}
