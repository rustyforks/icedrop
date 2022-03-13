use tokio::fs::File;
use tokio::io::Result;
use tokio::net::{TcpStream, ToSocketAddrs};
use tokio::runtime::Handle;

use crate::endpoint::{ControlMessage, Endpoint};
use crate::handlers::session::EndSessionHandler;
use crate::handlers::{file_transfer::FileTransferNextHandler, handshake::HandshakeRequestFrame};
use crate::proto::Frame;

pub struct Client {
    stream: Option<TcpStream>,
    file: Option<File>,
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
        })
    }

    pub fn set_file(&mut self, file: File) {
        self.file = Some(file);
    }

    pub async fn run(&mut self) {
        let mut endpoint = Endpoint::new(self.stream.take().unwrap());

        let file = self.file.take().unwrap();
        endpoint.add_handler(FileTransferNextHandler::new(file));
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
            client.run().await;
            Result::Ok(())
        })
        .unwrap();
    }
}
