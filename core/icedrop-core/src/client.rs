use tokio::fs::File;
use tokio::io::Result;
use tokio::net::{TcpStream, ToSocketAddrs};
use tokio::runtime::Handle;

use crate::endpoint::{ControlMessage, Endpoint};
use crate::handlers::file_transfer::{FileTransferEvent, FileTransferNextHandler};
use crate::handlers::handshake::HandshakeRequestFrame;
use crate::handlers::session::EndSessionHandler;
use crate::proto::Frame;

pub struct Client {
    stream: Option<TcpStream>,
    file: Option<File>,
    segment_sent_callback: Option<Box<dyn Fn(u32, usize) + Send>>,
    complete_callback: Option<Box<dyn Fn() + Send>>,
}

impl Client {
    pub async fn connect<A>(addr: A) -> Result<Self>
    where
        A: ToSocketAddrs,
    {
        let stream = TcpStream::connect(addr).await?;

        Ok(Self {
            stream: Some(stream),
            file: None,
            segment_sent_callback: None,
            complete_callback: None,
        })
    }

    pub fn set_file(&mut self, file: File) {
        self.file = Some(file);
    }

    pub fn set_segment_sent_callback<F>(&mut self, f: F)
    where
        F: Fn(u32, usize) + Send + 'static,
    {
        self.segment_sent_callback = Some(Box::new(f));
    }

    pub fn set_completed_callback<F>(&mut self, f: F)
    where
        F: Fn() + Send + 'static,
    {
        self.complete_callback = Some(Box::new(f));
    }

    pub async fn run(&mut self) {
        let mut endpoint = Endpoint::new(self.stream.take().unwrap());

        let file = self.file.take().unwrap();
        let mut file_transfer_next_handler = FileTransferNextHandler::new(file);
        let segment_sent_callback = self.segment_sent_callback.take();
        let complete_callback = self.complete_callback.take();
        if segment_sent_callback.is_some() || complete_callback.is_some() {
            file_transfer_next_handler.set_callback_fn(move |event| match event {
                FileTransferEvent::SegmentSent(segment_idx, bytes_sent) => {
                    if let Some(cb) = &segment_sent_callback {
                        cb.call((segment_idx, bytes_sent));
                    }
                }
                FileTransferEvent::Complete => {
                    if let Some(cb) = &complete_callback {
                        cb.call(());
                    }
                }
            });
        }
        endpoint.add_handler(file_transfer_next_handler);

        endpoint.add_handler(EndSessionHandler::new(endpoint.get_mailbox()));

        let mailbox = endpoint.get_mailbox();
        Handle::current().spawn(async move {
            let frame = HandshakeRequestFrame {
                name: "test".to_owned(),
            };
            mailbox
                .send(ControlMessage::SendFrame((
                    frame.frame_type(),
                    frame.to_bytes(),
                )))
                .await
                .unwrap();
        });

        let result = endpoint.run().await;
        if let Some(err) = result.err() {
            println!("error happened while talking to server: {:?}", err);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Client;

    use tokio::fs::File;
    use tokio::io::Result;
    use tokio::runtime::Runtime;

    #[test]
    fn simple_test() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let file = File::open("/Users/cyandev/Downloads/Detroit Become Human.mp4").await?;

            let mut client = Client::connect("127.0.0.1:8080").await?;
            client.set_file(file);
            client.set_segment_sent_callback(|segment_idx, bytes_sent| {
                println!(
                    "segment {} has sent and been received (total {} bytes sent)",
                    segment_idx, bytes_sent
                );
            });
            client.set_completed_callback(|| {
                println!("complete!");
            });
            client.run().await;
            Result::Ok(())
        })
        .unwrap();
    }
}
