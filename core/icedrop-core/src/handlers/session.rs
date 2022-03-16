use crate::{
    endpoint::EndpointHandle,
    proto::{Frame, FrameHandler, StreamReadHalf},
};

use std::error::Error;

use async_trait::async_trait;
use tokio::sync::mpsc::Sender;

#[derive(Debug)]
pub struct EndSessionFrame;

#[async_trait]
impl Frame for EndSessionFrame {
    fn frame_type(&self) -> u16 {
        return 99;
    }

    async fn parse<S>(
        frame_type: u16,
        _stream: &mut S,
    ) -> Option<Result<Self, Box<dyn Error + Send>>>
    where
        S: StreamReadHalf,
    {
        if frame_type != 99 {
            return None;
        }

        Some(Ok(EndSessionFrame))
    }

    fn to_bytes(&self) -> Vec<u8> {
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
