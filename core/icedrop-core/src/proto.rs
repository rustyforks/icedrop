use std::error::Error;
use std::fmt::Debug;

use async_trait::async_trait;
use tokio::io::AsyncReadExt;

pub trait StreamReadHalf: AsyncReadExt + Send + Unpin {}

#[async_trait]
pub trait Frame: Debug + Send + Sized {
    fn frame_type(&self) -> u16;

    async fn parse<S>(
        frame_type: u16,
        stream: &mut S,
    ) -> Option<Result<Self, Box<dyn Error + Send>>>
    where
        S: StreamReadHalf;

    fn to_bytes(&self) -> Vec<u8>;
}

#[async_trait]
pub trait FrameHandler {
    type IncomingFrame: Frame;

    async fn handle_frame(&mut self, frame: Self::IncomingFrame);
}

#[async_trait]
impl Frame for () {
    fn frame_type(&self) -> u16 {
        return 0;
    }

    async fn parse<S>(
        _frame_type: u16,
        _stream: &mut S,
    ) -> Option<Result<Self, Box<dyn Error + Send>>>
    where
        S: StreamReadHalf,
    {
        None
    }

    fn to_bytes(&self) -> Vec<u8> {
        Vec::new()
    }
}
