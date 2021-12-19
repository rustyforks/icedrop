#![feature(fn_traits)]

use std::sync::Arc;
use discovery::spawn_heartbeat_task;
use log::{info, error};
use tokio;
use tokio::sync::{mpsc, Mutex};

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
  
  let rs_join_handle = runtime.spawn(async {
    let server = transfer::ReceivingServer::default();

    info!("receiving server started!");
    server.start().await.unwrap();
  });

  runtime.block_on(async {
    let _ = rs_join_handle.await;
  });
  
  // runtime.block_on(async {
  //   let result: Option<()> = (async {
  //     let stream = tokio::net::TcpStream::connect("192.168.2.1:13996").await.ok()?;
  //     let shared_stream = Arc::new(Mutex::new(stream));

  //     spawn_heartbeat_task(shared_stream).await.ok()
  //   }).await;
  //   result.unwrap();
  // });
}

#[derive(Debug)]
pub enum SenderCommand {
  SendFile(String)
}

pub struct SenderController {
  tx: mpsc::Sender<SenderCommand>,
  rx: Option<mpsc::Receiver<SenderCommand>>,
}

impl SenderController {
  pub fn new() -> Self {
    let (tx, rx) = mpsc::channel::<SenderCommand>(1000);
    Self { tx, rx: Some(rx) }
  }

  pub fn dispatch_command(&self, cmd: SenderCommand) -> bool {
    return self.tx.blocking_send(cmd).is_ok();
  }

  pub fn take_command_rx(&mut self) -> mpsc::Receiver<SenderCommand> {
    return self.rx.take().unwrap();
  }
}

pub fn sender_main(endpoint: String, rx: mpsc::Receiver<SenderCommand>) {
  let runtime = tokio::runtime::Builder::new_current_thread()
    .enable_all()
    .build()
    .unwrap();
  
  let mut rx = rx; //controller.rx.take().unwrap();

  runtime.block_on(async move {
    let mut handle_cmd: Box<dyn FnMut(SenderCommand)> = Box::new(|item: SenderCommand| {
      info!("handle command: {:#?}", item);
      match item {
        SenderCommand::SendFile(path) => {
          let endpoint_clone = endpoint.clone();
          tokio::runtime::Handle::current().spawn(async {
            let client = transfer::client::FileSender::new(endpoint_clone).unwrap();
            let result = client.send_file(path).await;
            if let Err(err) = result {
              error!("{}", err);
            }
          });
        }
      }
    });

    info!("sender loop started");

    loop {
      let maybe_item = rx.recv().await;
      if let Some(item) = maybe_item {
        handle_cmd.as_mut().call_mut((item,));
      } else {
        return;
      }
    }
  });
}

#[cfg(test)]
mod tests {
  #[test]
  fn it_works() {
    assert_eq!(2 + 2, 4);
  }
}
