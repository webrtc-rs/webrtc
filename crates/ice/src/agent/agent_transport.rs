use super::*;
use crate::errors::*;

use async_trait::async_trait;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use util::Conn;

impl Agent {
    /// Connects to the remote agent, acting as the controlling ice agent.
    /// The method blocks until at least one ice candidate pair has successfully connected.
    pub async fn dial(
        &self,
        mut cancel_rx: mpsc::Receiver<()>,
        remote_ufrag: String,
        remote_pwd: String,
    ) -> Result<Arc<impl Conn>, Error> {
        let (on_connected_rx, agent_conn) = {
            let agent_internal = Arc::clone(&self.agent_internal);
            let mut ai = self.agent_internal.lock().await;
            ai.start_connectivity_checks(agent_internal, true, remote_ufrag, remote_pwd)
                .await?;
            (ai.on_connected_rx.take(), Arc::clone(&ai.agent_conn))
        };

        if let Some(mut on_connected_rx) = on_connected_rx {
            // block until pair selected
            tokio::select! {
                _ = on_connected_rx.recv() => {},
                _ = cancel_rx.recv() => {
                    return Err(ERR_CANCELED_BY_CALLER.to_owned());
                }
            }
        }
        Ok(agent_conn)
    }

    /// Connects to the remote agent, acting as the controlled ice agent.
    /// The method blocks until at least one ice candidate pair has successfully connected.
    pub async fn accept(
        &self,
        mut cancel_rx: mpsc::Receiver<()>,
        remote_ufrag: String,
        remote_pwd: String,
    ) -> Result<Arc<impl Conn>, Error> {
        let (on_connected_rx, agent_conn) = {
            let agent_internal = Arc::clone(&self.agent_internal);
            let mut ai = self.agent_internal.lock().await;
            ai.start_connectivity_checks(agent_internal, false, remote_ufrag, remote_pwd)
                .await?;
            (ai.on_connected_rx.take(), Arc::clone(&ai.agent_conn))
        };

        if let Some(mut on_connected_rx) = on_connected_rx {
            // block until pair selected
            tokio::select! {
                _ = on_connected_rx.recv() => {},
                _ = cancel_rx.recv() => {
                    return Err(ERR_CANCELED_BY_CALLER.to_owned());
                }
            }
        }

        Ok(agent_conn)
    }
}

pub(crate) struct AgentConn {
    pub(crate) selected_pair: Mutex<Option<Arc<CandidatePair>>>,
    pub(crate) checklist: Mutex<Vec<Arc<CandidatePair>>>,

    pub(crate) buffer: Buffer,
    pub(crate) bytes_received: AtomicUsize,
    pub(crate) bytes_sent: AtomicUsize,
    pub(crate) done: AtomicBool,
}

impl AgentConn {
    pub(crate) fn new() -> Self {
        Self {
            selected_pair: Mutex::new(None),
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
    pub(crate) async fn get_selected_pair(&self) -> Option<Arc<CandidatePair>> {
        let selected_pair = self.selected_pair.lock().await;
        selected_pair.clone()
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
    async fn connect(&self, _addr: SocketAddr) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::Other, "Not applicable"))
    }

    async fn recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        if self.done.load(Ordering::SeqCst) {
            return Err(io::Error::new(io::ErrorKind::Other, "Conn is closed"));
        }

        let n = match self.buffer.read(buf, None).await {
            Ok(n) => n,
            Err(err) => return Err(io::Error::new(io::ErrorKind::Other, err.to_string())),
        };
        self.bytes_received.fetch_add(n, Ordering::SeqCst);

        Ok(n)
    }

    async fn recv_from(&self, _buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        Err(io::Error::new(io::ErrorKind::Other, "Not applicable"))
    }

    async fn send(&self, buf: &[u8]) -> io::Result<usize> {
        if self.done.load(Ordering::SeqCst) {
            return Err(io::Error::new(io::ErrorKind::Other, "Conn is closed"));
        }

        if is_message(buf) {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                ERR_ICE_WRITE_STUN_MESSAGE.to_string(),
            ));
        }

        let result = if let Some(pair) = self.get_selected_pair().await {
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
            Err(err) => Err(io::Error::new(io::ErrorKind::Other, err.to_string())),
        }
    }

    async fn send_to(&self, _buf: &[u8], _target: SocketAddr) -> io::Result<usize> {
        Err(io::Error::new(io::ErrorKind::Other, "Not applicable"))
    }

    async fn local_addr(&self) -> io::Result<SocketAddr> {
        if let Some(pair) = self.get_selected_pair().await {
            Ok(pair.local.addr().await)
        } else {
            Err(io::Error::new(
                io::ErrorKind::AddrNotAvailable,
                "Addr Not Available",
            ))
        }
    }
}
