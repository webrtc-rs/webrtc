use std::io;
use std::sync::atomic::Ordering;

use arc_swap::ArcSwapOption;
use async_trait::async_trait;
use portable_atomic::AtomicBool;
use util::Conn;

use super::*;
use crate::error::*;

impl Agent {
    /// Connects to the remote agent, acting as the controlling ice agent.
    /// The method blocks until at least one ice candidate pair has successfully connected.
    ///
    /// The operation will be cancelled if `cancel_rx` either receives a message or its channel
    /// closes.
    pub async fn dial(
        &self,
        mut cancel_rx: mpsc::Receiver<()>,
        remote_ufrag: String,
        remote_pwd: String,
    ) -> Result<Arc<impl Conn>> {
        let (on_connected_rx, agent_conn) = {
            self.internal
                .start_connectivity_checks(true, remote_ufrag, remote_pwd)
                .await?;

            let mut on_connected_rx = self.internal.on_connected_rx.lock().await;
            (
                on_connected_rx.take(),
                Arc::clone(&self.internal.agent_conn),
            )
        };

        if let Some(mut on_connected_rx) = on_connected_rx {
            // block until pair selected
            tokio::select! {
                _ = on_connected_rx.recv() => {},
                _ = cancel_rx.recv() => {
                    return Err(Error::ErrCanceledByCaller);
                }
            }
        }
        Ok(agent_conn)
    }

    /// Connects to the remote agent, acting as the controlled ice agent.
    /// The method blocks until at least one ice candidate pair has successfully connected.
    ///
    /// The operation will be cancelled if `cancel_rx` either receives a message or its channel
    /// closes.
    pub async fn accept(
        &self,
        mut cancel_rx: mpsc::Receiver<()>,
        remote_ufrag: String,
        remote_pwd: String,
    ) -> Result<Arc<impl Conn>> {
        let (on_connected_rx, agent_conn) = {
            self.internal
                .start_connectivity_checks(false, remote_ufrag, remote_pwd)
                .await?;

            let mut on_connected_rx = self.internal.on_connected_rx.lock().await;
            (
                on_connected_rx.take(),
                Arc::clone(&self.internal.agent_conn),
            )
        };

        if let Some(mut on_connected_rx) = on_connected_rx {
            // block until pair selected
            tokio::select! {
                _ = on_connected_rx.recv() => {},
                _ = cancel_rx.recv() => {
                    return Err(Error::ErrCanceledByCaller);
                }
            }
        }

        Ok(agent_conn)
    }
}

pub(crate) struct AgentConn {
    pub(crate) selected_pair: ArcSwapOption<CandidatePair>,
    pub(crate) checklist: Mutex<Vec<Arc<CandidatePair>>>,

    pub(crate) buffer: Buffer,
    pub(crate) bytes_received: AtomicUsize,
    pub(crate) bytes_sent: AtomicUsize,
    pub(crate) done: AtomicBool,
}

impl AgentConn {
    pub(crate) fn new() -> Self {
        Self {
            selected_pair: ArcSwapOption::empty(),
            checklist: Mutex::new(vec![]),
            // Make sure the buffer doesn't grow indefinitely.
            // NOTE: We actually won't get anywhere close to this limit.
            // SRTP will constantly read from the endpoint and drop packets if it's full.
            buffer: Buffer::new(0, MAX_BUFFER_SIZE),
            bytes_received: AtomicUsize::new(0),
            bytes_sent: AtomicUsize::new(0),
            done: AtomicBool::new(false),
        }
    }
    pub(crate) fn get_selected_pair(&self) -> Option<Arc<CandidatePair>> {
        self.selected_pair.load().clone()
    }

    pub(crate) async fn get_best_available_candidate_pair(&self) -> Option<Arc<CandidatePair>> {
        let mut best: Option<&Arc<CandidatePair>> = None;

        let checklist = self.checklist.lock().await;
        for p in &*checklist {
            if p.state.load(Ordering::SeqCst) == CandidatePairState::Failed as u8 {
                continue;
            }

            if let Some(b) = &mut best {
                if b.priority() < p.priority() {
                    *b = p;
                }
            } else {
                best = Some(p);
            }
        }

        best.cloned()
    }

    pub(crate) async fn get_best_valid_candidate_pair(&self) -> Option<Arc<CandidatePair>> {
        let mut best: Option<&Arc<CandidatePair>> = None;

        let checklist = self.checklist.lock().await;
        for p in &*checklist {
            if p.state.load(Ordering::SeqCst) != CandidatePairState::Succeeded as u8 {
                continue;
            }

            if let Some(b) = &mut best {
                if b.priority() < p.priority() {
                    *b = p;
                }
            } else {
                best = Some(p);
            }
        }

        best.cloned()
    }

    /// Returns the number of bytes sent.
    pub fn bytes_sent(&self) -> usize {
        self.bytes_sent.load(Ordering::SeqCst)
    }

    /// Returns the number of bytes received.
    pub fn bytes_received(&self) -> usize {
        self.bytes_received.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl Conn for AgentConn {
    async fn connect(&self, _addr: SocketAddr) -> std::result::Result<(), util::Error> {
        Err(io::Error::new(io::ErrorKind::Other, "Not applicable").into())
    }

    async fn recv(&self, buf: &mut [u8]) -> std::result::Result<usize, util::Error> {
        if self.done.load(Ordering::SeqCst) {
            return Err(io::Error::new(io::ErrorKind::Other, "Conn is closed").into());
        }

        let n = match self.buffer.read(buf, None).await {
            Ok(n) => n,
            Err(err) => return Err(io::Error::new(io::ErrorKind::Other, err.to_string()).into()),
        };
        self.bytes_received.fetch_add(n, Ordering::SeqCst);

        Ok(n)
    }

    async fn recv_from(
        &self,
        buf: &mut [u8],
    ) -> std::result::Result<(usize, SocketAddr), util::Error> {
        if let Some(raddr) = self.remote_addr() {
            let n = self.recv(buf).await?;
            Ok((n, raddr))
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "Not applicable").into())
        }
    }

    async fn send(&self, buf: &[u8]) -> std::result::Result<usize, util::Error> {
        if self.done.load(Ordering::SeqCst) {
            return Err(io::Error::new(io::ErrorKind::Other, "Conn is closed").into());
        }

        if is_message(buf) {
            return Err(util::Error::Other("ErrIceWriteStunMessage".into()));
        }

        let result = if let Some(pair) = self.get_selected_pair() {
            pair.write(buf).await
        } else if let Some(pair) = self.get_best_available_candidate_pair().await {
            pair.write(buf).await
        } else {
            Ok(0)
        };

        match result {
            Ok(n) => {
                self.bytes_sent.fetch_add(buf.len(), Ordering::SeqCst);
                Ok(n)
            }
            Err(err) => Err(io::Error::new(io::ErrorKind::Other, err.to_string()).into()),
        }
    }

    async fn send_to(
        &self,
        _buf: &[u8],
        _target: SocketAddr,
    ) -> std::result::Result<usize, util::Error> {
        Err(io::Error::new(io::ErrorKind::Other, "Not applicable").into())
    }

    fn local_addr(&self) -> std::result::Result<SocketAddr, util::Error> {
        if let Some(pair) = self.get_selected_pair() {
            Ok(pair.local.addr())
        } else {
            Err(io::Error::new(io::ErrorKind::AddrNotAvailable, "Addr Not Available").into())
        }
    }

    fn remote_addr(&self) -> Option<SocketAddr> {
        self.get_selected_pair().map(|pair| pair.remote.addr())
    }

    async fn close(&self) -> std::result::Result<(), util::Error> {
        Ok(())
    }

    fn as_any(&self) -> &(dyn std::any::Any + Send + Sync) {
        self
    }
}
