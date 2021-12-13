use serde::{Serialize, Deserialize};
use crate::proto::{ToFrame, FromFrame};

pub(crate) const HANDSHAKE_FRAME_TYPE: u16 = 1;

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct HandshakeMessage {
  pub(crate) host_name: String
}

impl ToFrame for HandshakeMessage { }
impl<'a> FromFrame<'a> for HandshakeMessage { }
