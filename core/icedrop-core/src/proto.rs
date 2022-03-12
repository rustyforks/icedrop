use std::error::Error;
use std::fmt::Debug;

use async_trait::async_trait;
use tokio::io::AsyncReadExt;

pub trait Stream: AsyncReadExt + Send + Unpin {}

#[async_trait]
pub trait Frame: Debug + Send + Sized {
    fn frame_type(&self) -> u16;

    async fn parse<S>(
        frame_type: u16,
        stream: &mut S,
    ) -> Option<Result<Self, Box<dyn Error + Send>>>
    where
        S: Stream;

    fn to_bytes(&self) -> Vec<u8>;
}

#[async_trait]
pub trait FrameHandler {
    type IncomingFrame: Frame;
    type OutgoingFrame: Frame;

    async fn handle_frame(&mut self, frame: Self::IncomingFrame) -> Self::OutgoingFrame;
}
