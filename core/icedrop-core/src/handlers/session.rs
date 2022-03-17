use crate::{
    endpoint::EndpointHandle,
    proto::{Frame, FrameHandler, FrameParsingResult},
};

use async_trait::async_trait;

#[derive(Debug)]
pub struct EndSessionFrame;

impl Frame for EndSessionFrame {
    fn frame_type(&self) -> u16 {
        return 99;
    }

    fn try_parse(frame_type: u16, buf: Vec<u8>) -> FrameParsingResult<Self> {
        if frame_type != 99 {
            return FrameParsingResult::Skip(buf);
        }

        FrameParsingResult::Ok(EndSessionFrame)
    }

    fn to_bytes(self) -> Vec<u8> {
        Vec::new()
    }
}

pub struct EndSessionHandler {
    endpoint_handle: EndpointHandle,
}

impl EndSessionHandler {
    pub fn new(endpoint_handle: EndpointHandle) -> Self {
        Self { endpoint_handle }
    }
}

#[async_trait]
impl FrameHandler for EndSessionHandler {
    type IncomingFrame = EndSessionFrame;

    async fn handle_frame(&mut self, _frame: Self::IncomingFrame) {
        self.endpoint_handle.shutdown().await.unwrap();
    }
}
