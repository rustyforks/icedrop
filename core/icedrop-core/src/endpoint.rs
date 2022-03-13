use crate::proto::{Frame, FrameHandler, Stream};

use std::error::Error;
use std::fmt::Display;

use async_trait::async_trait;
use byteorder::{ByteOrder, LittleEndian};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::select;
use tokio::sync::mpsc::{channel, Receiver, Sender};

type FrameTuple = (u16, Vec<u8>);

impl Stream for TcpStream {}

#[async_trait]
trait AnyFrameHandler {
    async fn parse_and_handle_frame(
        &mut self,
        frame_type: u16,
        stream: &mut TcpStream,
    ) -> Option<Result<FrameTuple, Box<dyn Error + Send>>>;
}

struct AnyFrameHandlerImpl<H>
where
    H: FrameHandler + Send,
{
    inner: H,
}

#[async_trait]
impl<H> AnyFrameHandler for AnyFrameHandlerImpl<H>
where
    H: FrameHandler + Send,
{
    async fn parse_and_handle_frame(
        &mut self,
        frame_type: u16,
        stream: &mut TcpStream,
    ) -> Option<Result<FrameTuple, Box<dyn Error + Send>>> {
        let maybe_result = <H as FrameHandler>::IncomingFrame::parse(frame_type, stream).await;
        if maybe_result.is_none() {
            // This handler does not want to handle this frame.
            return None;
        }

        let result = maybe_result.unwrap();

        if result.is_ok() {
            let frame = result.unwrap();
            let fut = self.inner.handle_frame(frame);
            let outgoing_frame = fut.await;
            return Some(Ok((outgoing_frame.frame_type(), outgoing_frame.to_bytes())));
        } else {
            return Some(Err(result.unwrap_err()));
        }
    }
}

#[derive(Debug)]
pub struct EndpointError {
    message: String,
}

impl EndpointError {
    fn new(message: &str) -> Self {
        Self {
            message: message.to_owned(),
        }
    }
}

impl Display for EndpointError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message.as_str())
    }
}

impl Error for EndpointError {}
pub struct Endpoint {
    stream: TcpStream,
    handlers: Vec<Box<dyn AnyFrameHandler + Send>>,
    tx: Sender<FrameTuple>,
    rx: Option<Receiver<FrameTuple>>,
}

impl Endpoint {
    pub fn new(stream: TcpStream) -> Self {
        let (tx, rx) = channel(100);
        Self {
            stream,
            handlers: Vec::new(),
            tx,
            rx: Some(rx),
        }
    }

    pub fn add_handler<H>(&mut self, handler: H)
    where
        H: FrameHandler + Send + 'static,
    {
        let type_erased_handler: AnyFrameHandlerImpl<H> = AnyFrameHandlerImpl { inner: handler };
        self.handlers.push(Box::new(type_erased_handler));
    }

    pub fn get_mailbox(&self) -> Sender<FrameTuple> {
        return self.tx.clone();
    }
}

impl Endpoint {
    pub async fn run(&mut self) -> Result<(), Box<dyn Error>> {
        let mut rx = self.rx.take().unwrap();
        loop {
            let send_frame: Result<FrameTuple, _> = select! {
                frame = self.handle_incoming_frames() => frame,
                frame = rx.recv() => Ok(frame.unwrap()),
            };

            if send_frame.is_err() {
                return Err(send_frame.unwrap_err());
            }

            let send_frame = send_frame.unwrap();
            let mut frame_type_buf = [0 as u8; 2];
            LittleEndian::write_u16(&mut frame_type_buf, send_frame.0);

            println!("sending frame with type {}", send_frame.0);

            // TODO: add error handling.
            let _ = self.stream.write_all(&frame_type_buf).await;
            let _ = self.stream.write_all(&send_frame.1).await;
        }
    }

    async fn handle_incoming_frames(&mut self) -> Result<FrameTuple, Box<dyn Error + Send>> {
        let mut frame_type_buf = [0 as u8; 2];
        let read_size = self.stream.read_exact(&mut frame_type_buf).await;
        if let Ok(read_size) = read_size {
            if read_size != 2 {
                return Err(Box::new(EndpointError::new("Peer has closed unexpectedly")));
            }
        } else {
            return Err(Box::new(read_size.unwrap_err()));
        }

        let frame_type = LittleEndian::read_u16(&frame_type_buf);

        // Find the first handler that can handle the frame.
        for handler in &mut self.handlers {
            let maybe_result = handler
                .parse_and_handle_frame(frame_type, &mut self.stream)
                .await;
            if maybe_result.is_none() {
                continue;
            }
            let result = maybe_result.unwrap();
            if result.is_err() {
                return Err(result.unwrap_err());
            } else {
                let outgoing_frame = result.unwrap();
                return Ok(outgoing_frame);
            }
        }

        let msg = format!("No handlers can handle frame: {}", frame_type);
        return Err(Box::new(EndpointError::new(msg.as_str())));
    }
}
