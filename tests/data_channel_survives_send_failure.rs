//! Regression test for issue #776 — an unreliable data channel must survive failed sends.
//!
//! The report: a channel opened with `ordered: false, max_retransmits: Some(0)` moved to
//! `Closed` after a *single* failed transmission, when the reporter simulated loss with
//! `iptables -A OUTPUT -o lo -j DROP`. That contradicts RFC 8831 §6.1 — zero retransmits
//! plus unordered delivery is meant to be a UDP-like service, so a dropped packet should be
//! discarded, not terminal — and RFC 4960 §8, which fails an association only after the
//! `Association.Max.Retransmits` threshold, never on one loss.
//!
//! The mechanism the report describes is a socket write that *fails* rather than one that
//! is silently blackholed: on Linux an `OUTPUT ... -j DROP` rule makes `sendto()` for
//! locally-generated packets return `EPERM`, which is what surfaced as an error inside the
//! stack. That is reproduced here without root or `iptables` by wrapping the runtime's UDP
//! socket and returning `PermissionDenied` from `poll_send` once a flag is set — the same
//! errno, delivered deterministically, and only after the connection is already up.
//!
//! The assertion is deliberately narrow: **the channel is still `Open` shortly after the
//! failed sends**. It is not a claim that the connection survives indefinitely — with all
//! egress broken, ICE consent will eventually fail the connection, which is correct
//! behaviour and what other stacks do too. What must not happen is the terminal transition
//! on the first failure.

use anyhow::Result;
use bytes::BytesMut;
use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::task::{Context, Poll};
use std::time::Duration;

use webrtc::data_channel::{DataChannelEvent, RTCDataChannelInit, RTCDataChannelState};
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCIceGatheringState,
    RTCPeerConnectionState,
};
use webrtc::runtime::{
    AsyncInterval, AsyncTcpListener, AsyncTcpStream, AsyncUdpSocket, JoinHandle, RecvMeta, Runtime,
    Sender, Transmit, channel,
};

mod common;
use common::{block_on, runtime, sleep, timeout};

// ── A runtime whose UDP sends can be made to fail on demand ────────────────────
//
// Everything delegates to the runtime under test; only `wrap_udp_socket` is decorated.

#[derive(Debug)]
struct FailingSendRuntime {
    inner: Arc<dyn Runtime>,
    fail: Arc<AtomicBool>,
    failed: Arc<AtomicUsize>,
}

impl Runtime for FailingSendRuntime {
    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>) -> Box<dyn JoinHandle> {
        self.inner.spawn(future)
    }

    fn spawn_reactor(
        &self,
        reactor_pool_size: usize,
        future: Pin<Box<dyn Future<Output = ()> + Send>>,
    ) -> Box<dyn JoinHandle> {
        self.inner.spawn_reactor(reactor_pool_size, future)
    }

    fn wrap_udp_socket(&self, socket: std::net::UdpSocket) -> io::Result<Arc<dyn AsyncUdpSocket>> {
        Ok(Arc::new(FailingSendSocket {
            inner: self.inner.wrap_udp_socket(socket)?,
            fail: self.fail.clone(),
            failed: self.failed.clone(),
        }))
    }

    fn wrap_tcp_listener(
        &self,
        listener: std::net::TcpListener,
    ) -> io::Result<Arc<dyn AsyncTcpListener>> {
        self.inner.wrap_tcp_listener(listener)
    }

    fn connect_tcp<'a>(
        &'a self,
        remote_addr: SocketAddr,
    ) -> Pin<Box<dyn Future<Output = io::Result<Arc<dyn AsyncTcpStream>>> + Send + 'a>> {
        self.inner.connect_tcp(remote_addr)
    }

    fn resolve_host<'a>(
        &'a self,
        host: &'a str,
    ) -> Pin<Box<dyn Future<Output = io::Result<Vec<SocketAddr>>> + Send + 'a>> {
        self.inner.resolve_host(host)
    }

    fn sleep(&self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        self.inner.sleep(duration)
    }

    fn interval(&self, period: Duration) -> Box<dyn AsyncInterval> {
        self.inner.interval(period)
    }

    fn block_on(&self, future: Pin<Box<dyn Future<Output = ()> + '_>>) {
        self.inner.block_on(future)
    }

    fn name(&self) -> &'static str {
        "failing-send"
    }
}

#[derive(Debug)]
struct FailingSendSocket {
    inner: Arc<dyn AsyncUdpSocket>,
    fail: Arc<AtomicBool>,
    failed: Arc<AtomicUsize>,
}

impl AsyncUdpSocket for FailingSendSocket {
    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }

    fn poll_send(&self, cx: &mut Context<'_>, transmit: &Transmit<'_>) -> Poll<io::Result<usize>> {
        if self.fail.load(Ordering::SeqCst) {
            // What `iptables -A OUTPUT -j DROP` gives a local sender on Linux.
            self.failed.fetch_add(1, Ordering::Relaxed);
            return Poll::Ready(Err(io::Error::from(io::ErrorKind::PermissionDenied)));
        }
        self.inner.poll_send(cx, transmit)
    }

    fn poll_recv(
        &self,
        cx: &mut Context<'_>,
        bufs: &mut [io::IoSliceMut<'_>],
        meta: &mut [RecvMeta],
    ) -> Poll<io::Result<usize>> {
        self.inner.poll_recv(cx, bufs, meta)
    }

    fn max_gso_segments(&self) -> usize {
        self.inner.max_gso_segments()
    }

    fn max_gro_segments(&self) -> usize {
        self.inner.max_gro_segments()
    }
}

// ── Handlers ──────────────────────────────────────────────────────────────────

struct Handler {
    gather_tx: Sender<()>,
    connected_tx: Sender<()>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for Handler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            let _ = self.gather_tx.try_send(());
        }
    }
    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        if state == RTCPeerConnectionState::Connected {
            let _ = self.connected_tx.try_send(());
        }
    }
}

#[test]
fn test_unreliable_data_channel_survives_failed_sends() {
    block_on(run()).unwrap();
}

async fn run() -> Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .is_test(true)
        .try_init()
        .ok();

    let base = runtime();
    let fail = Arc::new(AtomicBool::new(false));
    let failed = Arc::new(AtomicUsize::new(0));
    let sender_runtime: Arc<dyn Runtime> = Arc::new(FailingSendRuntime {
        inner: base.clone(),
        fail: fail.clone(),
        failed: failed.clone(),
    });

    let (snd_gather_tx, mut snd_gather_rx) = channel::<()>(1);
    let (snd_conn_tx, mut snd_conn_rx) = channel::<()>(1);
    let (rcv_gather_tx, mut rcv_gather_rx) = channel::<()>(1);
    let (rcv_conn_tx, mut rcv_conn_rx) = channel::<()>(1);

    // ── Sender, on the runtime whose sends we can break ───────────────────────
    let sender_pc = PeerConnectionBuilder::new()
        .with_handler(Arc::new(Handler {
            gather_tx: snd_gather_tx,
            connected_tx: snd_conn_tx,
        }))
        .with_runtime(sender_runtime)
        .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
        .build()
        .await?;

    // Exactly the configuration from the issue: UDP-like, per RFC 8831 §6.1.
    let dc = sender_pc
        .create_data_channel(
            "unreliable",
            Some(RTCDataChannelInit {
                ordered: false,
                max_retransmits: Some(0),
                ..Default::default()
            }),
        )
        .await?;

    let (open_tx, mut open_rx) = channel::<()>(1);
    {
        let dc = dc.clone();
        base.spawn(Box::pin(async move {
            while let Some(event) = dc.poll().await {
                match event {
                    DataChannelEvent::OnOpen => {
                        let _ = open_tx.try_send(());
                    }
                    DataChannelEvent::OnClose => break,
                    _ => {}
                }
            }
        }));
    }

    let offer = sender_pc.create_offer(None).await?;
    sender_pc.set_local_description(offer).await?;
    let _ = timeout(Duration::from_secs(5), snd_gather_rx.recv()).await;
    let offer_sdp = sender_pc
        .local_description()
        .await
        .expect("sender local description");

    // ── Receiver, on the unmodified runtime ───────────────────────────────────
    let receiver_pc = PeerConnectionBuilder::new()
        .with_handler(Arc::new(Handler {
            gather_tx: rcv_gather_tx,
            connected_tx: rcv_conn_tx,
        }))
        .with_runtime(base.clone())
        .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
        .build()
        .await?;

    receiver_pc.set_remote_description(offer_sdp).await?;
    let answer = receiver_pc.create_answer(None).await?;
    receiver_pc.set_local_description(answer).await?;
    let _ = timeout(Duration::from_secs(5), rcv_gather_rx.recv()).await;
    let answer_sdp = receiver_pc
        .local_description()
        .await
        .expect("receiver local description");
    sender_pc.set_remote_description(answer_sdp).await?;

    timeout(Duration::from_secs(15), snd_conn_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("timeout: sender connect"))?;
    timeout(Duration::from_secs(5), rcv_conn_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("timeout: receiver connect"))?;
    timeout(Duration::from_secs(10), open_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("timeout: data channel open"))?;

    assert_eq!(
        dc.ready_state().await?,
        RTCDataChannelState::Open,
        "precondition: channel open before breaking the network"
    );

    // ── Break every outgoing datagram, then send ──────────────────────────────
    fail.store(true, Ordering::SeqCst);

    for _ in 0..5 {
        // `send` is allowed to fail — the application learns the datagram did not go out.
        // What it must not do is tear the channel down.
        let _ = dc.send(BytesMut::from(&b"payload"[..])).await;
        sleep(Duration::from_millis(50)).await;
    }

    // Give any erroneous close a generous chance to land. This is well inside ICE's own
    // consent/timeout window, so a channel that closes here closed because of the send
    // failures, not because connectivity was declared lost.
    sleep(Duration::from_secs(2)).await;

    // Teeth: without this the test would pass vacuously if the send path stopped calling
    // `poll_send` at all, or if the injection were wired up wrongly.
    let failed_sends = failed.load(Ordering::Relaxed);
    assert!(
        failed_sends > 0,
        "no send actually failed — the injection did not take effect, so the assertion \
         below would prove nothing"
    );

    let state = dc.ready_state().await?;
    assert_eq!(
        state,
        RTCDataChannelState::Open,
        "issue #776: an unordered / max_retransmits(0) channel must not go terminal on \
         failed sends — RFC 8831 §6.1 makes it a UDP-like service, and RFC 4960 §8 fails an \
         association only at the retransmit threshold. Observed state: {state:?}"
    );

    Ok(())
}
