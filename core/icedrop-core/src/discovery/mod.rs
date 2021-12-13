use std::sync::Arc;
use tokio::{self, task::JoinHandle, net::TcpStream};
use tokio::sync::Mutex;
use tokio::io::AsyncWriteExt;
use crate::proto::ToFrame;

pub(crate) mod server;
mod msg;

pub(crate) fn spawn_heartbeat_task(client: Arc<Mutex<TcpStream>>) -> JoinHandle<()> {
  tokio::spawn(async move {
    let _ = (async move {
      let handshake_msg = msg::HandshakeMessage {
        host_name: "darwin".to_owned()
      };
      let frame = handshake_msg.to_frame(msg::HANDSHAKE_FRAME_TYPE)?;
      let mut buf = Vec::<u8>::new();
      frame.write_to_buffer(&mut buf);

      let mut client_guard = client.lock().await;
      client_guard.write(&buf).await.ok()?;

      Some(())
    }).await;
  })
}