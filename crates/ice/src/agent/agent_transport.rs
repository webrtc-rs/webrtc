use super::*;
use crate::errors::*;

use async_trait::async_trait;
use std::io;
use std::sync::atomic::Ordering;
use util::Conn;

impl Agent {
    // Dial connects to the remote agent, acting as the controlling ice agent.
    // Dial blocks until at least one ice candidate pair has successfully connected.
    pub async fn dial(
        &self,
        remote_ufrag: String,
        remote_pwd: String,
    ) -> Result<Arc<Mutex<impl Conn>>, Error> {
        let on_connected_rx = {
            let agent_internal = Arc::clone(&self.agent_internal);
            let mut ai = self.agent_internal.lock().await;
            ai.start_connectivity_checks(agent_internal, true, remote_ufrag, remote_pwd)
                .await?;
            ai.on_connected_rx.take()
        };

        if let Some(mut on_connected_rx) = on_connected_rx {
            // block until pair selected
            tokio::select! {
                _ = on_connected_rx.recv() => {},
            }
        }
        Ok(Arc::clone(&self.agent_internal))
    }

    // Accept connects to the remote agent, acting as the controlled ice agent.
    // Accept blocks until at least one ice candidate pair has successfully connected.
    pub async fn accept(
        &self,
        remote_ufrag: String,
        remote_pwd: String,
    ) -> Result<Arc<Mutex<impl Conn>>, Error> {
        let on_connected_rx = {
            let agent_internal = Arc::clone(&self.agent_internal);
            let mut ai = self.agent_internal.lock().await;
            ai.start_connectivity_checks(agent_internal, false, remote_ufrag, remote_pwd)
                .await?;
            ai.on_connected_rx.take()
        };

        if let Some(mut on_connected_rx) = on_connected_rx {
            // block until pair selected
            tokio::select! {
                _ = on_connected_rx.recv() => {},
            }
        }

        Ok(Arc::clone(&self.agent_internal))
    }
}

impl AgentInternal {
    // bytes_sent returns the number of bytes sent
    pub fn bytes_sent(&self) -> usize {
        self.bytes_sent.load(Ordering::SeqCst)
    }

    // bytes_received returns the number of bytes received
    pub fn bytes_received(&self) -> usize {
        self.bytes_received.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl Conn for AgentInternal {
    async fn connect(&self, _addr: SocketAddr) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::Other, "Not applicable"))
    }

    async fn recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        if self.done_tx.is_none() {
            return Err(io::Error::new(io::ErrorKind::Other, "Conn is closed"));
        }

        let mut n = 0;
        if let Some(buffer) = &self.buffer {
            n = match buffer.read(buf, None).await {
                Ok(n) => n,
                Err(err) => return Err(io::Error::new(io::ErrorKind::Other, err.to_string())),
            };
            self.bytes_received.fetch_add(n, Ordering::SeqCst);
        }

        Ok(n)
    }

    async fn recv_from(&self, _buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        Err(io::Error::new(io::ErrorKind::Other, "Not applicable"))
    }

    async fn send(&self, buf: &[u8]) -> io::Result<usize> {
        if self.done_tx.is_none() {
            return Err(io::Error::new(io::ErrorKind::Other, "Conn is closed"));
        }

        if is_message(buf) {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                ERR_ICE_WRITE_STUN_MESSAGE.to_string(),
            ));
        }

        let result = if let Some(pair) = self.get_selected_pair() {
            pair.write(buf).await
        } else if let Some(pair) = self.get_best_available_candidate_pair() {
            pair.write(buf).await
        } else {
            Err(ERR_NO_CANDIDATE_PAIRS.to_owned())
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

    fn local_addr(&self) -> io::Result<SocketAddr> {
        if let Some(pair) = self.get_selected_pair() {
            Ok(pair.local.addr())
        } else {
            Err(io::Error::new(
                io::ErrorKind::AddrNotAvailable,
                "Addr Not Available",
            ))
        }
    }
}
