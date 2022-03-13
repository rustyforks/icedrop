use crate::proto::{Frame, FrameHandler, StreamReadHalf};

use std::error::Error;
use std::fmt::Display;
use std::sync::Arc;

use async_trait::async_trait;
use byteorder::{ByteOrder, LittleEndian};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio::select;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::Mutex;

type FrameTuple = (u16, Vec<u8>);

#[derive(Debug)]
pub enum ControlMessage {
    SendFrame(FrameTuple),
    Shutdown,
}

impl StreamReadHalf for OwnedReadHalf {}

#[async_trait]
trait AnyFrameHandler {
    async fn parse_and_handle_frame(
        &mut self,
        frame_type: u16,
        stream: &mut OwnedReadHalf,
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
        stream: &mut OwnedReadHalf,
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
    stream_rd: Arc<Mutex<OwnedReadHalf>>,
    stream_wr: Arc<Mutex<OwnedWriteHalf>>,
    handlers: Option<Vec<Box<dyn AnyFrameHandler + Send>>>,
    tx: Sender<ControlMessage>,
    rx: Option<Receiver<ControlMessage>>,
}

impl Endpoint {
    pub fn new(stream: TcpStream) -> Self {
        let (rd_half, wr_half) = stream.into_split();
        let (tx, rx) = channel(100);
        Self {
            stream_rd: Arc::new(Mutex::new(rd_half)),
            stream_wr: Arc::new(Mutex::new(wr_half)),
            handlers: Some(Vec::new()),
            tx,
            rx: Some(rx),
        }
    }

    pub fn add_handler<H>(&mut self, handler: H)
    where
        H: FrameHandler + Send + 'static,
    {
        if let Some(handlers) = &mut self.handlers {
            let type_erased_handler: AnyFrameHandlerImpl<H> =
                AnyFrameHandlerImpl { inner: handler };
            handlers.push(Box::new(type_erased_handler));
        } else {
            panic!("Cannot add handlers after the endpoint runs!");
        }
    }

    pub fn get_mailbox(&self) -> Sender<ControlMessage> {
        return self.tx.clone();
    }
}

impl Endpoint {
    pub async fn run(&mut self) -> Result<(), Box<dyn Error>> {
        let mut handlers = self.handlers.take().unwrap();
        let stream_rd_clone = Arc::clone(&self.stream_rd);
        let stream_wr_clone = Arc::clone(&self.stream_wr);
        let net_fut = async move {
            loop {
                let fut =
                    Self::handle_incoming_frames(&stream_rd_clone, &stream_wr_clone, &mut handlers);
                if let Err(err) = fut.await {
                    return Err(err);
                }
            }
            #[allow(unreachable_code)]
            Ok(()) // Help the compiler to infer a return type.
        };

        let mut rx = self.rx.take().unwrap();
        let stream_wr_clone = Arc::clone(&self.stream_wr);
        let msg_fut = async move {
            loop {
                let msg = rx.recv().await.unwrap();
                match msg {
                    ControlMessage::SendFrame(frame) => {
                        let mut stream_wr_locked = stream_wr_clone.lock().await;
                        Self::send_frame(&mut *stream_wr_locked, frame).await;
                    }
                    ControlMessage::Shutdown => return Ok(()),
                }
            }
            #[allow(unreachable_code)]
            Err(Box::new(EndpointError::new(panic!()))) // Help the compiler to infer a return type.
        };

        // TODO: add lifecycle management.
        let result = select! {
            result = net_fut => { result },
            _ = msg_fut => { Ok(()) },
        };
        if let Err(err) = result {
            return Err(err);
        }
        Ok(())
    }

    async fn send_frame(stream_wr: &mut OwnedWriteHalf, frame: FrameTuple) {
        // Assembly the frame.
        let mut send_buf: Vec<u8> = Vec::with_capacity(2 + frame.1.len());
        send_buf.resize(2, 0);
        LittleEndian::write_u16(&mut send_buf, frame.0);
        send_buf.extend(frame.1);

        #[cfg(debug_assertions)]
        println!(
            "sending frame with type {} ({} bytes)",
            frame.0,
            send_buf.len()
        );

        // TODO: add error handling.
        let _ = stream_wr.write_all(&send_buf).await;
    }

    async fn handle_incoming_frames(
        stream_rd: &Arc<Mutex<OwnedReadHalf>>,
        stream_wr: &Arc<Mutex<OwnedWriteHalf>>,
        handlers: &mut Vec<Box<dyn AnyFrameHandler + Send>>,
    ) -> Result<(), Box<dyn Error + Send>> {
        let mut stream_rd_locked = stream_rd.lock().await;

        // Read the frame type.
        let mut frame_type_buf = [0 as u8; 2];
        let read_size = stream_rd_locked.read_exact(&mut frame_type_buf).await;

        if let Ok(read_size) = read_size {
            if read_size != 2 {
                return Err(Box::new(EndpointError::new("Peer has closed unexpectedly")));
            }
        } else {
            return Err(Box::new(read_size.unwrap_err()));
        }

        let frame_type = LittleEndian::read_u16(&frame_type_buf);

        // Find the first handler that can handle the frame.
        for handler in handlers {
            let maybe_result = handler
                .parse_and_handle_frame(frame_type, &mut *stream_rd_locked)
                .await;

            if maybe_result.is_none() {
                continue;
            }

            let result = maybe_result.unwrap();
            if result.is_err() {
                return Err(result.unwrap_err());
            } else {
                let outgoing_frame = result.unwrap();
                if outgoing_frame.0 != 0 {
                    let mut stream_wr_locked = stream_wr.lock().await;
                    Self::send_frame(&mut *stream_wr_locked, outgoing_frame).await;
                    drop(stream_wr_locked); // Let other futures have the change to poll with stream.
                }
                return Ok(());
            }
        }

        let msg = format!("No handlers can handle frame: {}", frame_type);
        return Err(Box::new(EndpointError::new(msg.as_str())));
    }
}
