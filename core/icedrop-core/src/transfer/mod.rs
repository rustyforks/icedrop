use std::cmp;
use log::{info, error};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use crate::proto::{self, FromFrame};

pub(crate) mod client;
mod msg;

pub(crate) struct ReceivingServer {

}

impl Default for ReceivingServer {
  fn default() -> Self {
    Self {

    }
  }
}

impl ReceivingServer {
  pub(crate) async fn start(&self) -> tokio::io::Result<()> {
    let listener = TcpListener::bind("0.0.0.0:13997").await?;
    
    loop {
      let (socket, _) = listener.accept().await?;
      tokio::spawn(async move {
        info!("new conn from {:?}", socket.peer_addr());
        ReceivingServer::handle_connection(socket).await;
      });
    }
  }

  async fn handle_connection(mut socket: TcpStream) -> Option<()> {
    let frame = proto::Frame::read_from(&mut socket).await?;
    if frame.frame_type != msg::PRESEND_FRAME_TYPE {
      error!("unexpected frame type: {}", frame.frame_type);
      return None;
    }

    let presend_msg = msg::PresendMessage::from_frame(&frame)?;
    info!("receiving {} (total {} bytes)...", presend_msg.file_name, presend_msg.total_bytes);

    let mut to_recv = presend_msg.total_bytes as usize;
    while to_recv > 0 {
      // Receive 4 KB chunk each time.
      let buf_len = cmp::min(to_recv, 1024 * 4);
      let mut buf = vec![0 as u8; buf_len];
      let n = socket.read(&mut buf).await.ok()?;
      if n == 0 && to_recv > 0 {
        error!("unexpected eof while receiving");
        return None;
      }

      info!("{} bytes remain", to_recv);
      to_recv -= n;
    }
    
    socket.shutdown().await.ok()?;
    Some(())
  }
}