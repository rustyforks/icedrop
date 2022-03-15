use std::ffi::c_void;
use std::fs::File as StdFile;
use std::os::unix::prelude::FromRawFd;
use std::path::Path;

use tokio::fs::File;
use tokio::runtime;
use tokio::sync::mpsc::{channel, Receiver, Sender};

use icedrop_core::Client;

pub trait ClientRequest {
    fn execute(self: Box<Self>, client: &mut IcedropClient);
}

pub struct IcedropClient {
    req_rx: Receiver<Box<dyn ClientRequest>>,
    req_tx: Sender<Box<dyn ClientRequest>>,
}

impl IcedropClient {
    pub fn new() -> Self {
        let (tx, rx) = channel(100);
        Self {
            req_rx: rx,
            req_tx: tx,
        }
    }

    pub fn run_in_current_thread(&mut self) {
        let rt = runtime::Builder::new_current_thread()
            .thread_name("icedrop-client")
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(async {
            loop {
                let req = self.req_rx.recv().await.unwrap();
                req.execute(self);
            }
        });
    }

    pub fn send_request<R>(&self, req: R)
    where
        R: ClientRequest + 'static,
    {
        let tx = self.req_tx.clone();
        tx.blocking_send(Box::new(req)).ok().unwrap(); // TODO: this may fail.
    }
}

pub struct UserInfoPtr(pub *mut c_void);

impl Clone for UserInfoPtr {
    fn clone(&self) -> Self {
        Self(self.0)
    }
}

unsafe impl Send for UserInfoPtr {}

pub struct SendFileRequest {
    pub remote_addr: String,
    pub file: StdFile,
    pub user_info: UserInfoPtr,
    pub segment_sent_callback: Option<Box<dyn Fn(*mut c_void, u32, usize) + Send>>,
    pub completed_callback: Option<Box<dyn Fn(*mut c_void) + Send>>,
}

impl SendFileRequest {
    pub fn new<A, F>(remote_addr: A, local_file_path: F) -> Result<Self, std::io::Error>
    where
        A: Into<String>,
        F: AsRef<Path>,
    {
        let file = StdFile::open(local_file_path)?;
        Ok(SendFileRequest {
            remote_addr: remote_addr.into(),
            file,
            user_info: UserInfoPtr(std::ptr::null_mut()),
            segment_sent_callback: None,
            completed_callback: None,
        })
    }

    pub fn with_fd<A>(remote_addr: A, file_fd: i32) -> Self
    where
        A: Into<String>,
    {
        let file = unsafe { StdFile::from_raw_fd(file_fd) };
        SendFileRequest {
            remote_addr: remote_addr.into(),
            file,
            user_info: UserInfoPtr(std::ptr::null_mut()),
            segment_sent_callback: None,
            completed_callback: None,
        }
    }
}

impl ClientRequest for SendFileRequest {
    fn execute(self: Box<Self>, _client: &mut IcedropClient) {
        runtime::Handle::current().spawn(async move {
            let client = Client::connect(self.remote_addr).await;
            if client.is_err() {
                // TODO: add error handling.
                return;
            }

            let mut client = client.unwrap();
            client.set_file(File::from_std(self.file));
            if let Some(cb) = self.segment_sent_callback {
                let user_info = self.user_info.clone();
                client.set_segment_sent_callback(move |segment_idx, bytes_sent| {
                    cb.call((user_info.0, segment_idx, bytes_sent));
                });
            }
            if let Some(cb) = self.completed_callback {
                let user_info = self.user_info.clone();
                client.set_completed_callback(move || {
                    cb.call((user_info.0,));
                });
            }
            client.run().await;
        });
    }
}
