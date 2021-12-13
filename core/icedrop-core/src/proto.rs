use std::io::Cursor;
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use serde::{Serialize, Deserialize};

pub(crate) struct Frame {
  pub(crate) frame_type: u16,
  pub(crate) payload: String
}

impl Frame {
  pub(crate) fn new<S>(frame_type: u16, payload: S) -> Self
  where
    S: ToString
  {
    Self {
      frame_type: frame_type,
      payload: payload.to_string(),
    }
  }

  pub(crate) async fn read_from(stream: &mut TcpStream) -> Option<Self> {
    let mut buf = [0 as u8; 6];
    let n = stream.read(&mut buf).await.ok()?;

    if n < 6 {
      // Cannot form a valid frame.
      return None;
    }

    let mut cursor = Cursor::new(buf);
    let frame_type = ReadBytesExt::read_u16::<BigEndian>(&mut cursor).ok()?;
    let payload_len = ReadBytesExt::read_u32::<BigEndian>(&mut cursor).ok()?;

    if payload_len == 0 {
      return Some(Self {
        frame_type: frame_type,
        payload: String::new(),
      });
    }

    let mut buf = vec![0 as u8; payload_len as usize];
    let n = stream.read(&mut buf).await.ok()?;
    if payload_len as usize != n {
      return None;
    }

    Some(Self {
      frame_type: frame_type,
      payload: String::from_utf8(buf).ok()?,
    })
  }

  pub(crate) fn write_to_buffer(&self, buf: &mut Vec<u8>) {
    buf.write_u16::<BigEndian>(self.frame_type).unwrap();
    buf.write_u32::<BigEndian>(self.payload.len() as u32).unwrap();
    buf.extend(self.payload.as_bytes());
  }
}

pub(crate) trait ToFrame: Serialize {
  fn to_frame(&self, frame_type: u16) -> Option<Frame> {
    let json_payload = serde_json::to_string(&self).ok()?;
    Some(Frame {
      frame_type: frame_type,
      payload: json_payload,
    })
  }
}

pub(crate) trait FromFrame<'a>: Deserialize<'a> {
  fn from_frame(frame: &'a Frame) -> Option<Self> {
    serde_json::from_str(&frame.payload).ok()?
  }
}
