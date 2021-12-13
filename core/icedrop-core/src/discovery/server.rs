use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use std::net::Ipv4Addr;
use log::{debug, info, error};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use super::msg;
use crate::proto::{self, FromFrame};

struct HostInfo {
  host_name: String,
  last_active_time: Instant,
}

struct DiscoveryServerInner {
  hosts: HashMap<Ipv4Addr, HostInfo>,
}

impl DiscoveryServerInner {
  fn new() -> Self {
    Self {
      hosts: HashMap::new(),
    }
  }
}

pub(crate) struct DiscoveryServer {
  inner: Arc<Mutex<DiscoveryServerInner>>,
}

impl Default for DiscoveryServer {
  fn default() -> Self {
    Self {
      inner: Arc::new(Mutex::new(DiscoveryServerInner::new())),
    }
  }
}

impl DiscoveryServer {
  pub(crate) async fn start(&self) -> tokio::io::Result<()> {
    let listener = TcpListener::bind("0.0.0.0:13996").await?;
    
    loop {
      let (socket, _) = listener.accept().await?;
      let inner_clone = Arc::clone(&self.inner);
      tokio::spawn(async move {
        info!("new conn from {:?}", socket.peer_addr());
        let result = DiscoveryServer::handle_connection(inner_clone, socket).await;
        if result.is_none() {
          error!("error occurred while handling connection");
        }
      });
    }
  }

  async fn handle_connection(inner: Arc<Mutex<DiscoveryServerInner>>, mut client: TcpStream) -> Option<()> {
    let frame = proto::Frame::read_from(&mut client).await?;
    if frame.frame_type != msg::HANDSHAKE_FRAME_TYPE {
      error!("unexpected frame type: {}", frame.frame_type);
      return None;
    }

    let handshake_msg = msg::HandshakeMessage::from_frame(&frame)?;
    
    // Looks like we successfully received a handshake frame, register the
    // host and start the heartbeat checking.
    let mut inner_guard = inner.lock().await;
    let remote_ip: std::net::Ipv4Addr = {
      if let std::net::SocketAddr::V4(ipv4) = client.peer_addr().unwrap() {
        Some(ipv4.ip().clone())
      } else {
        None
      }
    }.unwrap();
    inner_guard.hosts.insert(remote_ip, HostInfo {
      host_name: handshake_msg.host_name,
      last_active_time: Instant::now(),
    });
    drop(inner_guard);

    info!("host registered, start polling: {:?}", remote_ip);

    loop {
      tokio::time::sleep(std::time::Duration::from_secs(5)).await;
      debug!("wake up, check the connectivity: {:?}", remote_ip);

      if let Some(frame) = proto::Frame::read_from(&mut client).await {
        debug!("still alive: {:?}", remote_ip);
      } else {
        info!("host dropped, unregister it: {:?}", remote_ip);
        break;
      }
    }

    // Unregister the host.
    let mut inner_guard = inner.lock().await;
    inner_guard.hosts.remove(&remote_ip);
    drop(inner_guard);
    
    return Some(());
  }
}