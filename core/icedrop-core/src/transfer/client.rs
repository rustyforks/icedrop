use std::cmp;
use std::path::Path;
use std::io::{ErrorKind as IoErrorKind, Error as IoError};
use std::net::{SocketAddr, ToSocketAddrs};
use log::info;
use tokio::io::{Result, AsyncReadExt, AsyncWriteExt};
use tokio::fs::File;
use tokio::net::TcpStream;
use super::msg;
use crate::proto::ToFrame;

pub(crate) struct FileSender {
  endpoint: SocketAddr
}

impl FileSender {
  pub(crate) fn new<A: ToSocketAddrs>(endpoint: A) -> Option<Self> {
    Some(Self {
      endpoint: endpoint.to_socket_addrs().ok()?.next()?
    })
  }

  pub(crate) async fn send_file<F>(&self, path: F) -> Result<()>
  where
    F: AsRef<Path>
  {
    let file_name = path
      .as_ref()
      .file_name()
      .ok_or(IoError::new(IoErrorKind::Other, "failed to extract file name"))?
      .to_str()
      .ok_or(IoError::new(IoErrorKind::Other, "failed to convert OsStr to &str"))?
      .to_owned();

    let mut file = File::open(path).await?;
    let file_len = file.metadata().await?.len();

    info!("prepare to connect to: {:?}", self.endpoint);
    let mut socket = TcpStream::connect(self.endpoint).await?;
    info!("connected to host: {:?}", socket);
    
    // Prepare presend message.
    let presend_msg = msg::PresendMessage {
      file_name: file_name,
      total_bytes: file_len
    };
    let frame = presend_msg
      .to_frame(msg::PRESEND_FRAME_TYPE)
      .ok_or(IoError::new(IoErrorKind::Other, "tbd"))?;
    let mut buf = Vec::<u8>::new();
    frame.write_to_buffer(&mut buf);

    // Send presend message.
    socket.write_all(&buf).await?;

    // Actually send the file.
    let mut to_send = file_len as usize;
    while to_send > 0 {
      // Send 4 KB chunk each time.
      let buf_len = cmp::min(to_send, 1024 * 4);
      let mut buf = vec![0 as u8; buf_len];
      let n = file.read(&mut buf).await?;
      if n == 0 {
        break;
      }

      socket.write_all(&buf).await?;

      info!("{} bytes remain", to_send);
      to_send -= n;
    }

    Ok(())
  }
}