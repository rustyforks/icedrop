use crate::{
    endpoint::EndpointHandle,
    proto::{Frame, FrameHandler, FrameParsingResult},
};

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

    fn try_parse(frame_type: u16, buf: Vec<u8>) -> FrameParsingResult<Self> {
        if frame_type != 1 {
            return FrameParsingResult::Skip(buf);
        }

        let size = LittleEndian::read_u32(&buf) as usize;
        let name = String::from_utf8_lossy(&buf[4..(4 + size)]);
        return FrameParsingResult::Ok(Self {
            name: name.into_owned(),
        });
    }

    fn to_bytes(self) -> Vec<u8> {
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

    fn try_parse(frame_type: u16, buf: Vec<u8>) -> FrameParsingResult<Self> {
        if frame_type != 2 {
            return FrameParsingResult::Skip(buf);
        }

        FrameParsingResult::Ok(HandshakeResponseFrame)
    }

    fn to_bytes(self) -> Vec<u8> {
        Vec::new()
    }
}

pub struct HandshakeHandler {
    endpoint_handle: EndpointHandle,
}

impl HandshakeHandler {
    pub fn new(endpoint_handle: EndpointHandle) -> Self {
        Self { endpoint_handle }
    }
}

#[async_trait]
impl FrameHandler for HandshakeHandler {
    type IncomingFrame = HandshakeRequestFrame;

    async fn handle_frame(&mut self, frame: Self::IncomingFrame) {
        println!("{}", frame.name);
        self.endpoint_handle
            .send_frame(HandshakeResponseFrame)
            .await
            .unwrap();
    }
}
