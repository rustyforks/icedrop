use crate::endpoint::Endpoint;
use crate::handlers::{file_transfer::FileTransferReceivingHandler, handshake::HandshakeHandler};

use tokio::io::Result;
use tokio::net::{TcpListener, TcpStream, ToSocketAddrs};
use tokio::runtime::Handle;

struct Server {
    listener: TcpListener,
}

impl Server {
    pub async fn bind<A>(addr: A) -> Result<Self>
    where
        A: ToSocketAddrs,
    {
        let listener = TcpListener::bind(addr).await?;
        Ok(Self { listener })
    }

    pub async fn run(&mut self) {
        loop {
            match self.listener.accept().await {
                Ok((stream, addr)) => {
                    println!("new client: {:?}", addr);
                    Self::serve_client(stream);
                }
                Err(e) => {
                    println!("could not accept new client: {:?}", e);
                }
            }
        }
    }

    fn serve_client(stream: TcpStream) {
        Handle::current().spawn(async {
            let mut endpoint = Endpoint::new(stream);
            endpoint.add_handler(HandshakeHandler::new(endpoint.handle()));
            let endpoint_handle = endpoint.handle();
            endpoint.add_handler(
                FileTransferReceivingHandler::new(endpoint_handle, "/var/tmp/icedrop").await,
            );
            let result = endpoint.run().await;
            if let Some(err) = result.err() {
                println!("error happened while serving a client: {:?}", err);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::Server;

    use tokio::io::Result;
    use tokio::runtime::Runtime;

    #[test]
    fn simple_test() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let mut server = Server::bind("0.0.0.0:8080").await?;
            server.run().await;
            Result::Ok(())
        })
        .unwrap();
    }
}
