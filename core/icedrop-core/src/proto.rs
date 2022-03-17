use std::error::Error;
use std::fmt::Debug;

use async_trait::async_trait;

pub enum FrameParsingResult<F> {
    Ok(F),
    Skip(Vec<u8>),
    Err(Box<dyn Error + Send>),
}

impl<F> FrameParsingResult<F> {
    pub(crate) fn unwrap_buf(self) -> Vec<u8> {
        match self {
            Self::Skip(val) => val,
            _ => panic!("called `FrameParsingResult::unwrap_buf()` on a non-skip value"),
        }
    }
}

pub trait Frame: Debug + Send + Sized {
    fn frame_type(&self) -> u16;

    fn try_parse(frame_type: u16, buf: Vec<u8>) -> FrameParsingResult<Self>;

    fn to_bytes(self) -> Vec<u8>;
}

#[async_trait]
pub trait FrameHandler {
    type IncomingFrame: Frame;

    async fn handle_frame(&mut self, frame: Self::IncomingFrame);
}
