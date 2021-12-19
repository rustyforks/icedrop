use serde::{Serialize, Deserialize};
use crate::proto::{ToFrame, FromFrame};

pub(crate) const PRESEND_FRAME_TYPE: u16 = 200;

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct PresendMessage {
  pub(crate) file_name: String,
  pub(crate) total_bytes: u64
}

impl ToFrame for PresendMessage { }
impl<'a> FromFrame<'a> for PresendMessage { }
