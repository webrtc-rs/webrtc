//! A STUN write on one local socket that never completes must not stall the agent. Without the
//! bound in `AgentInternal::send_stun`, the connectivity-check loop awaited that write forever, so
//! the controlling agent never nominated and the peer stayed in `checking`.

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use stun::message::*;
use tokio::net::UdpSocket;
use util::Conn;

use super::*;
use crate::candidate::candidate_base::*;
use crate::candidate::candidate_host::*;

/// A UDP conn whose writes never complete, like a socket that stopped reporting writable.
struct NeverWritableConn {
    local: SocketAddr,
}

#[async_trait]
impl Conn for NeverWritableConn {
    async fn connect(&self, _addr: SocketAddr) -> util::Result<()> {
        Ok(())
    }
    async fn recv(&self, _buf: &mut [u8]) -> util::Result<usize> {
        std::future::pending().await
    }
    async fn recv_from(&self, _buf: &mut [u8]) -> util::Result<(usize, SocketAddr)> {
        std::future::pending().await
    }
    async fn send(&self, _buf: &[u8]) -> util::Result<usize> {
        std::future::pending().await
    }
    async fn send_to(&self, _buf: &[u8], _target: SocketAddr) -> util::Result<usize> {
        std::future::pending().await
    }
    fn local_addr(&self) -> util::Result<SocketAddr> {
        Ok(self.local)
    }
    fn remote_addr(&self) -> Option<SocketAddr> {
        None
    }
    async fn close(&self) -> util::Result<()> {
        Ok(())
    }
    fn as_any(&self) -> &(dyn std::any::Any + Send + Sync) {
        self
    }
}

fn host_candidate(
    addr: SocketAddr,
    conn: Option<Arc<dyn Conn + Send + Sync>>,
) -> Arc<dyn Candidate + Send + Sync> {
    Arc::new(
        CandidateHostConfig {
            base_config: CandidateBaseConfig {
                network: "udp".to_owned(),
                address: addr.ip().to_string(),
                port: addr.port(),
                component: 1,
                conn,
                ..Default::default()
            },
            ..Default::default()
        }
        .new_candidate_host()
        .unwrap(),
    )
}

fn binding_request() -> Message {
    let mut msg = Message::new();
    msg.build(&[Box::new(BINDING_REQUEST), Box::new(TransactionId::new())])
        .unwrap();
    msg
}

#[tokio::test]
async fn test_send_stun_on_a_socket_that_never_becomes_writable_returns_within_the_bound(
) -> Result<()> {
    let a = Agent::new(AgentConfig::default()).await?;
    let stuck = host_candidate(
        "127.0.0.1:9".parse().unwrap(),
        Some(Arc::new(NeverWritableConn {
            local: "127.0.0.1:9".parse().unwrap(),
        })),
    );
    let remote = host_candidate("127.0.0.1:10".parse().unwrap(), None);

    let started = Instant::now();
    let sent = tokio::time::timeout(
        Duration::from_secs(2),
        a.internal.send_stun(&binding_request(), &stuck, &remote),
    )
    .await;
    assert!(sent.is_ok(), "send_stun never returned");
    assert!(
        started.elapsed() < STUN_SEND_TIMEOUT * 5,
        "send_stun took {:?}",
        started.elapsed()
    );

    a.close().await?;
    Ok(())
}

#[tokio::test]
async fn test_never_writable_local_socket_does_not_stop_checks_on_other_pairs() -> Result<()> {
    let a = Agent::new(AgentConfig::default()).await?;

    // Pair 1 (pinged first): a local socket whose writes never complete.
    let stuck = host_candidate(
        "127.0.0.1:9".parse().unwrap(),
        Some(Arc::new(NeverWritableConn {
            local: "127.0.0.1:9".parse().unwrap(),
        })),
    );
    // Pair 2: a working local socket and a remote that records what it receives.
    let ok_sock: Arc<dyn Conn + Send + Sync> = Arc::new(UdpSocket::bind("127.0.0.1:0").await?);
    let ok_local = host_candidate(ok_sock.local_addr()?, Some(ok_sock));
    let receiver = UdpSocket::bind("127.0.0.1:0").await?;
    let remote_ok = host_candidate(receiver.local_addr()?, None);
    let remote_stuck = host_candidate("127.0.0.1:10".parse().unwrap(), None);

    a.internal.add_pair(stuck, remote_stuck).await;
    a.internal.add_pair(ok_local, remote_ok).await;

    // Runs on its own task, like the agent's connectivity-check loop.
    let ai = Arc::clone(&a.internal);
    let pinging = tokio::spawn(async move { ai.ping_all_candidates().await });

    let mut buf = [0u8; 1500];
    let received = tokio::time::timeout(Duration::from_secs(2), receiver.recv_from(&mut buf)).await;
    assert!(
        received.is_ok(),
        "the working pair was never checked while the first pair's write hung"
    );
    let finished = tokio::time::timeout(Duration::from_secs(2), pinging).await;
    assert!(finished.is_ok(), "ping_all_candidates never returned");

    a.close().await?;
    Ok(())
}
