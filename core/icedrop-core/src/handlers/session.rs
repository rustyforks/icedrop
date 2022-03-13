use crate::proto::{Frame, Stream};

use std::error::Error;

use async_trait::async_trait;

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
        S: Stream,
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
