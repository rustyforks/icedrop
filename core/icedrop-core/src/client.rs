use tokio::io::Result;
use tokio::net::{TcpStream, ToSocketAddrs};
use tokio::runtime::Handle;

use crate::endpoint::Endpoint;
use crate::handlers::HandshakeRequestFrame;
use crate::proto::Frame;

pub struct Client {
    stream: Option<TcpStream>,
}

impl Client {
    pub async fn connect<A>(addr: A) -> Result<Self>
    where
        A: ToSocketAddrs,
    {
        let stream = TcpStream::connect(addr).await?;

        Ok(Self {
            stream: Some(stream),
        })
    }

    pub async fn run(&mut self) {
        let mut endpoint = Endpoint::new(self.stream.take().unwrap());

        let mailbox = endpoint.get_mailbox();
        Handle::current().spawn(async move {
            let frame = HandshakeRequestFrame {
                name: "test".to_owned(),
            };
            mailbox
                .send((frame.frame_type(), frame.to_bytes()))
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

    use tokio::io::Result;
    use tokio::runtime::Runtime;

    #[test]
    fn simple_test() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let mut client = Client::connect("127.0.0.1:8080").await?;
            client.run().await;
            Result::Ok(())
        })
        .unwrap();
    }
}
