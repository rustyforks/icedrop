use std::sync::Arc;
use discovery::spawn_heartbeat_task;
use log::info;
use tokio::{self, sync::Mutex};

mod proto;
mod discovery;
mod transfer;

pub fn discovery_server_main() {
  let runtime = tokio::runtime::Builder::new_current_thread()
    .enable_all()
    .build()
    .unwrap();

  runtime.block_on(async {
    let server = discovery::server::DiscoveryServer::default();

    info!("discovery server started!");
    server.start().await.unwrap();
  });
}

pub fn receiver_server_main() {
  let runtime = tokio::runtime::Builder::new_current_thread()
    .enable_all()
    .build()
    .unwrap();
  
  runtime.block_on(async {
    let result: Option<()> = (async {
      let stream = tokio::net::TcpStream::connect("192.168.2.1:13996").await.ok()?;
      let shared_stream = Arc::new(Mutex::new(stream));

      spawn_heartbeat_task(shared_stream).await.ok()
    }).await;
    result.unwrap();
  });
}

#[cfg(test)]
mod tests {
  #[test]
  fn it_works() {
    assert_eq!(2 + 2, 4);
  }
}
