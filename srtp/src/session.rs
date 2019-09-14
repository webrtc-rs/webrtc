use util::Error;

use crate::context::*;
use crate::stream::*;

use std::collections::HashMap;
use std::sync::Mutex;

use tokio::net::UdpSocket;
use tokio::sync::mpsc;

pub mod session_srtcp;
pub mod session_srtp;

pub trait StreamSession {
    fn close(&mut self) -> Result<(), Error>;
    fn write(&mut self, buf: &[u8]) -> Result<usize, Error>;
    fn decrypt(&mut self, buf: &[u8]) -> Result<(), Error>;
}

pub(crate) struct Session {
    //localContextMutex           sync.Mutex
    local_context: Option<Context>,
    remote_context: Option<Context>,

    new_stream_tx: Option<mpsc::Sender<Box<dyn ReadStream>>>,
    new_stream_rx: mpsc::Receiver<Box<dyn ReadStream>>,

    started_tx: Option<mpsc::Sender<()>>,
    started_rx: mpsc::Receiver<()>,
    closed_tx: Option<mpsc::Sender<()>>,
    closed_rx: mpsc::Receiver<()>,

    read_streams_closed: bool,
    read_streams: Mutex<HashMap<u32, Box<dyn ReadStream>>>,

    next_conn: UdpSocket,
    //log logging.LeveledLogger
}

impl Session {
    pub fn new(next_conn: UdpSocket) -> Self {
        let (new_stream_tx, new_stream_rx) = mpsc::channel(1);
        let (started_tx, started_rx) = mpsc::channel(1);
        let (closed_tx, closed_rx) = mpsc::channel(1);

        Session {
            local_context: None,
            remote_context: None,

            new_stream_tx: Some(new_stream_tx),
            new_stream_rx,

            started_tx: Some(started_tx),
            started_rx,
            closed_tx: Some(closed_tx),
            closed_rx,

            read_streams_closed: false,
            read_streams: Mutex::new(HashMap::new()),

            next_conn,
        }
    }

    async fn get_or_create_read_stream(
        &mut self,
        ssrc: u32,
        child: impl StreamSession,
        proto: fn() -> Box<dyn ReadStream>,
    ) -> Option<&Box<dyn ReadStream>> {
        let read_streams = self.read_streams.get_mut().unwrap();

        if self.read_streams_closed {
            return None;
        }

        if !read_streams.contains_key(&ssrc) {
            let mut r = proto();
            if r.init(ssrc).await.is_err() {
                return None;
            }
            read_streams.insert(ssrc, r);
        }

        read_streams.get(&ssrc)
    }

    pub(crate) fn remove_read_stream(&mut self, ssrc: u32) {
        let read_streams = self.read_streams.get_mut().unwrap();

        if self.read_streams_closed {
            return;
        }

        read_streams.remove(&ssrc);
    }

    async fn close(&mut self) -> Result<(), Error> {
        self.closed_rx.recv().await;

        Ok(())
    }

    async fn run(&mut self, child: impl StreamSession) -> Result<(), Error> {
        self.started_tx.take(); //drop started_x

        let mut buf: Vec<u8> = vec![0; 8192];
        loop {
            let result = self.next_conn.recv_from(&mut buf).await;
            match result {
                Ok((len, remote)) => {
                    /*match &child {
                        Session::SessionSRTP(s) => {}
                        Session::SessionSRTCP(s) => {}
                    };*/
                }
                Err(err) => {
                    self.new_stream_tx.take();
                    self.closed_tx.take();
                    self.read_streams_closed = true; //TODO: mutex?
                    return Err(err.into());
                }
            }
        }
    }

    async fn start(
        &mut self,
        local_master_key: Vec<u8>,
        local_master_salt: Vec<u8>,
        remote_master_key: Vec<u8>,
        remote_master_salt: Vec<u8>,
        profile: ProtectionProfile,
        child: impl StreamSession,
    ) -> Result<(), Error> {
        self.local_context = Some(Context::new(local_master_key, local_master_salt, profile)?);
        self.remote_context = Some(Context::new(
            remote_master_key,
            remote_master_salt,
            profile,
        )?);

        // This starts the server task.
        self.run(child).await?;

        Ok(())
    }
}
