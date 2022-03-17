use crate::proto::{Frame, FrameHandler, FrameParsingResult};

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

struct FrameWithHeader<F>
where
    F: Frame,
{
    frame: F,
}

impl<F> FrameWithHeader<F>
where
    F: Frame,
{
    fn to_bytes(self) -> Vec<u8> {
        let frame_type = self.frame.frame_type();
        let payload = self.frame.to_bytes();

        let mut frame_type_buf = [0 as u8; 2];
        LittleEndian::write_u16(&mut frame_type_buf, frame_type);

        let mut frame_len_buf = [0 as u8; 4];
        LittleEndian::write_u32(&mut frame_len_buf, payload.len() as u32);

        let mut buf = Vec::with_capacity(6 + payload.len());
        buf.extend(frame_type_buf);
        buf.extend(frame_len_buf);
        buf.extend(payload);

        buf
    }
}

pub enum AnyFrameHandlerResult {
    Ok,
    Skip(Vec<u8>),
    Err(Box<dyn Error + Send>),
}

#[async_trait]
trait AnyFrameHandler {
    async fn parse_and_handle_frame(
        &mut self,
        frame_type: u16,
        frame_payload: Vec<u8>,
    ) -> AnyFrameHandlerResult;
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
        frame_payload: Vec<u8>,
    ) -> AnyFrameHandlerResult {
        let parsing_result =
            <H as FrameHandler>::IncomingFrame::try_parse(frame_type, frame_payload);
        if let FrameParsingResult::Skip(payload) = parsing_result {
            return AnyFrameHandlerResult::Skip(payload);
        } else if let FrameParsingResult::Err(err) = parsing_result {
            return AnyFrameHandlerResult::Err(err);
        } else if let FrameParsingResult::Ok(frame) = parsing_result {
            let fut = self.inner.handle_frame(frame);
            fut.await;
            return AnyFrameHandlerResult::Ok;
        }
        unreachable!()
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

pub struct EndpointHandle {
    stream_wr: Arc<Mutex<OwnedWriteHalf>>,
    shutdown_tx: Sender<()>,
}

impl EndpointHandle {
    pub async fn send_frame<F>(&self, frame: F) -> Result<(), Box<dyn Error>>
    where
        F: Frame,
    {
        #[cfg(debug_assertions)]
        let frame_type = frame.frame_type();

        let frame_with_header = FrameWithHeader { frame };
        let buf = frame_with_header.to_bytes();

        #[cfg(debug_assertions)]
        println!(
            "sending frame with type {} ({} bytes)",
            frame_type,
            buf.len()
        );

        let mut stream_wr_locked = self.stream_wr.lock().await;
        stream_wr_locked.write_all(&buf).await?;
        Ok(())
    }

    pub async fn shutdown(&self) -> Result<(), Box<dyn Error>> {
        self.shutdown_tx.try_send(())?;
        Ok(())
    }
}

impl Clone for EndpointHandle {
    fn clone(&self) -> Self {
        Self {
            stream_wr: Arc::clone(&self.stream_wr),
            shutdown_tx: self.shutdown_tx.clone(),
        }
    }
}

pub struct Endpoint {
    stream_rd: Arc<Mutex<OwnedReadHalf>>,
    stream_wr: Arc<Mutex<OwnedWriteHalf>>,
    handlers: Option<Vec<Box<dyn AnyFrameHandler + Send>>>,
    shutdown_tx: Sender<()>,
    shutdown_rx: Receiver<()>,
}

impl Endpoint {
    pub fn new(stream: TcpStream) -> Self {
        let (rd_half, wr_half) = stream.into_split();
        let (tx, rx) = channel(1);
        Self {
            stream_rd: Arc::new(Mutex::new(rd_half)),
            stream_wr: Arc::new(Mutex::new(wr_half)),
            handlers: Some(Vec::new()),
            shutdown_tx: tx,
            shutdown_rx: rx,
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

    pub fn handle(&self) -> EndpointHandle {
        EndpointHandle {
            stream_wr: Arc::clone(&self.stream_wr),
            shutdown_tx: self.shutdown_tx.clone(),
        }
    }
}

impl Endpoint {
    pub async fn run(mut self) -> Result<(), Box<dyn Error>> {
        let mut handlers = self.handlers.take().unwrap();
        let stream_rd_clone = Arc::clone(&self.stream_rd);
        let net_fut = async move {
            loop {
                let fut = Self::handle_incoming_frames(&stream_rd_clone, &mut handlers);
                if let Err(err) = fut.await {
                    return Err(err);
                }
            }
            #[allow(unreachable_code)]
            Ok(()) // Help the compiler to infer a return type.
        };

        let mut shutdown_rx = self.shutdown_rx;
        let result = select! {
            result = net_fut => { result },
            _ = shutdown_rx.recv() => { Ok(()) },
        };
        if let Err(err) = result {
            return Err(err);
        }
        Ok(())
    }

    async fn handle_incoming_frames(
        stream_rd: &Arc<Mutex<OwnedReadHalf>>,
        handlers: &mut Vec<Box<dyn AnyFrameHandler + Send>>,
    ) -> Result<(), Box<dyn Error + Send>> {
        let mut stream_rd_locked = stream_rd.lock().await;

        // Read the frame type.
        let mut frame_header_buf = [0 as u8; 6];
        let read_size = stream_rd_locked.read_exact(&mut frame_header_buf).await;

        if let Ok(read_size) = read_size {
            if read_size != frame_header_buf.len() {
                return Err(Box::new(EndpointError::new("Peer has closed unexpectedly")));
            }
        } else {
            return Err(Box::new(read_size.unwrap_err()));
        }

        let frame_type = LittleEndian::read_u16(&frame_header_buf);
        let frame_len = LittleEndian::read_u32(&frame_header_buf[2..]) as usize;

        let mut frame_buf = Vec::with_capacity(frame_len);
        unsafe {
            frame_buf.set_len(frame_len as usize);
        }
        let read_size = stream_rd_locked.read_exact(&mut frame_buf).await;

        if read_size.is_err() {
            return Err(Box::new(read_size.unwrap_err()));
        } else if read_size.unwrap() != frame_len {
            return Err(Box::new(EndpointError::new("Peer has closed unexpectedly")));
        }

        // Find the first handler that can handle the frame.
        for handler in handlers {
            let maybe_result = handler.parse_and_handle_frame(frame_type, frame_buf).await;

            if let AnyFrameHandlerResult::Skip(buf) = maybe_result {
                frame_buf = buf;
                continue;
            } else if let AnyFrameHandlerResult::Err(err) = maybe_result {
                return Err(err);
            } else {
                return Ok(());
            }
        }

        let msg = format!("No handlers can handle frame: {}", frame_type);
        return Err(Box::new(EndpointError::new(msg.as_str())));
    }
}
