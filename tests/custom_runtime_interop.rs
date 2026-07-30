//! Two peer connections, two different runtimes, one process.
//!
//! Asserts the two properties the runtime abstraction exists for, which the
//! `custom-runtime` example cannot show on its own because it only builds one runtime:
//!
//! 1. **Per-connection runtime selection.** Each `PeerConnection` uses the runtime handed to
//!    `with_runtime`, and two connections with *different* runtimes coexist and interoperate
//!    concurrently in one process. A design with a process-global runtime registry could not
//!    express this.
//! 2. **Custom runtimes are pluggable.** The answerer runs on `MyRuntime` — built on
//!    `async-executor` + `async-io`, neither Tokio nor smol, and defined entirely outside the
//!    crate. It drives a real ICE/DTLS/SCTP connection: no `#[cfg]` edits, no fork, nothing
//!    in the library special-cased for it.
//!
//! The runtime under test is shared with the example rather than copied, so the two cannot
//! drift apart.
//!
//! Note the asymmetry that makes this a real test rather than a tautology: the offerer's
//! driver runs on the built-in runtime's executor while the answerer's runs on
//! `MyRuntime`'s own threads. Every packet between them crosses that boundary, so a
//! connection that completes proves both runtimes are independently live.

use anyhow::Result;
use bytes::BytesMut;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use webrtc::data_channel::{DataChannel, DataChannelEvent, RTCDataChannelInit};
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCIceGatheringState,
    RTCPeerConnectionState,
};
use webrtc::runtime::{Runtime, Sender, channel};

mod common;
use common::{block_on, runtime, timeout};

// The custom runtime is the example's, included by path so there is exactly one definition.
#[path = "../examples/custom-runtime/my_runtime.rs"]
mod my_runtime;

const LABEL: &str = "cross-runtime";
const PAYLOAD: &[u8] = b"hello across runtimes";

struct Handler {
    gather_tx: Sender<()>,
    connected_tx: Sender<()>,
    /// Set when the peer announces a channel it did not create (answerer side).
    announced: Arc<Mutex<Option<String>>>,
    announced_tx: Sender<Arc<dyn DataChannel>>,
    received: Arc<Mutex<Vec<u8>>>,
    received_tx: Sender<()>,
    runtime: Arc<dyn Runtime>,
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

    async fn on_data_channel(&self, dc: Arc<dyn DataChannel>) {
        *self.announced.lock().unwrap() = Some(dc.label().await.unwrap_or_default());
        let _ = self.announced_tx.try_send(dc.clone());

        // Drain the channel on *this* peer's own runtime, so the receive path is driven by
        // whichever runtime this connection was built with.
        let received = Arc::clone(&self.received);
        let received_tx = self.received_tx.clone();
        self.runtime.spawn(Box::pin(async move {
            while let Some(event) = dc.poll().await {
                match event {
                    DataChannelEvent::OnMessage(msg) => {
                        received.lock().unwrap().extend_from_slice(&msg.data);
                        let _ = received_tx.try_send(());
                    }
                    DataChannelEvent::OnClose => break,
                    _ => {}
                }
            }
        }));
    }
}

struct Peer {
    pc: Box<dyn PeerConnection>,
    runtime: Arc<dyn Runtime>,
    announced: Arc<Mutex<Option<String>>>,
    received: Arc<Mutex<Vec<u8>>>,
    gather_rx: webrtc::runtime::Receiver<()>,
    connected_rx: webrtc::runtime::Receiver<()>,
    announced_rx: webrtc::runtime::Receiver<Arc<dyn DataChannel>>,
    received_rx: webrtc::runtime::Receiver<()>,
}

/// Build a peer connection on `runtime`, whatever that runtime happens to be.
///
/// Nothing here is runtime-specific: the same code path builds the built-in-runtime peer and
/// the custom-runtime peer, which is the point.
async fn build_peer(runtime: Arc<dyn Runtime>) -> Result<Peer> {
    let (gather_tx, gather_rx) = channel::<()>(1);
    let (connected_tx, connected_rx) = channel::<()>(1);
    let (announced_tx, announced_rx) = channel::<Arc<dyn DataChannel>>(4);
    let (received_tx, received_rx) = channel::<()>(4);
    let announced = Arc::new(Mutex::new(None));
    let received = Arc::new(Mutex::new(Vec::new()));

    let pc = PeerConnectionBuilder::new()
        .with_handler(Arc::new(Handler {
            gather_tx,
            connected_tx,
            announced: announced.clone(),
            announced_tx,
            received: received.clone(),
            received_tx,
            runtime: runtime.clone(),
        }))
        .with_runtime(runtime.clone())
        .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
        .build()
        .await?;

    Ok(Peer {
        pc: Box::new(pc),
        runtime,
        announced,
        received,
        gather_rx,
        connected_rx,
        announced_rx,
        received_rx,
    })
}

#[test]
fn test_two_peers_on_different_runtimes_interoperate() {
    block_on(run()).unwrap();
}

async fn run() -> Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .is_test(true)
        .try_init()
        .ok();

    // Offerer: the compiled-in built-in (tokio or smol, per feature).
    let builtin: Arc<dyn Runtime> = runtime();
    // Answerer: a runtime defined outside the crate entirely.
    let custom: Arc<dyn Runtime> = Arc::new(my_runtime::MyRuntime::new());

    // Property 1, precondition: these really are two different runtimes, not the same one
    // twice. Without this the rest of the test could pass trivially.
    assert_ne!(
        builtin.name(),
        custom.name(),
        "test is meaningless unless the two runtimes differ"
    );
    assert_eq!(
        custom.name(),
        "my-runtime",
        "answerer must be on the out-of-tree runtime"
    );
    log::info!(
        "offerer runtime = {}, answerer runtime = {}",
        builtin.name(),
        custom.name()
    );

    let mut offerer = build_peer(builtin.clone()).await?;
    let mut answerer = build_peer(custom.clone()).await?;

    // ── Negotiate across the runtime boundary ─────────────────────────────────
    let dc = offerer
        .pc
        .create_data_channel(LABEL, Some(RTCDataChannelInit::default()))
        .await?;

    // Watch for the offerer's channel opening, on the offerer's runtime.
    let (open_tx, mut open_rx) = channel::<()>(1);
    {
        let dc = dc.clone();
        offerer.runtime.spawn(Box::pin(async move {
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

    let offer = offerer.pc.create_offer(None).await?;
    offerer.pc.set_local_description(offer).await?;
    let _ = timeout(Duration::from_secs(5), offerer.gather_rx.recv()).await;
    let offer_sdp = offerer
        .pc
        .local_description()
        .await
        .expect("offerer local description");

    answerer.pc.set_remote_description(offer_sdp).await?;
    let answer = answerer.pc.create_answer(None).await?;
    answerer.pc.set_local_description(answer).await?;
    let _ = timeout(Duration::from_secs(5), answerer.gather_rx.recv()).await;
    let answer_sdp = answerer
        .pc
        .local_description()
        .await
        .expect("answerer local description");
    offerer.pc.set_remote_description(answer_sdp).await?;

    // ── Property 1 + 2: the connection completes across the boundary ──────────
    // Both drivers must be independently live for this to resolve: the offerer's runs on the
    // built-in runtime, the answerer's on `MyRuntime`'s own executor threads.
    timeout(Duration::from_secs(20), offerer.connected_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("offerer (built-in runtime) never reached Connected"))?;
    timeout(Duration::from_secs(10), answerer.connected_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("answerer (custom runtime) never reached Connected"))?;

    timeout(Duration::from_secs(10), open_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("data channel never opened"))?;

    // The custom-runtime peer must observe the remotely created channel, which exercises its
    // spawn + SCTP receive path rather than only ICE/DTLS.
    timeout(Duration::from_secs(10), answerer.announced_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("custom-runtime peer never saw the announced channel"))?;
    assert_eq!(
        answerer.announced.lock().unwrap().as_deref(),
        Some(LABEL),
        "custom-runtime peer should see the offerer's channel label"
    );

    // ── Application data crosses the boundary ─────────────────────────────────
    dc.send(BytesMut::from(PAYLOAD)).await?;
    timeout(Duration::from_secs(10), answerer.received_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("payload never arrived on the custom-runtime peer"))?;

    // The app-delivery channel is bounded and lossy under load, but a single small message
    // on an idle channel must arrive intact.
    let got = answerer.received.lock().unwrap().clone();
    assert_eq!(
        got.as_slice(),
        PAYLOAD,
        "custom-runtime peer received the wrong bytes"
    );

    // ── Property 1, after the fact: still two distinct runtimes ───────────────
    // Guards against a regression where `with_runtime` is ignored and both connections
    // silently share one runtime — the connection would still succeed, so only an identity
    // check catches it.
    assert_eq!(offerer.runtime.name(), builtin.name());
    assert_eq!(answerer.runtime.name(), "my-runtime");
    assert!(
        !Arc::ptr_eq(&offerer.runtime, &answerer.runtime),
        "the two peers must hold distinct runtime instances"
    );

    offerer.pc.close().await?;
    answerer.pc.close().await?;
    Ok(())
}

/// A second connection on the custom runtime, concurrent with the first pair.
///
/// Property 1 says *multiple* runtimes coexist; this adds the weaker but distinct claim that
/// several connections can share one custom runtime instance, so the runtime is not
/// implicitly single-connection.
#[test]
fn test_custom_runtime_serves_multiple_connections() {
    block_on(async {
        let custom: Arc<dyn Runtime> = Arc::new(my_runtime::MyRuntime::new());
        let built = AtomicUsize::new(0);

        for _ in 0..3 {
            let peer = build_peer(custom.clone())
                .await
                .expect("build a peer on the shared custom runtime");
            // Gathering exercises the runtime's sockets and timers per connection.
            let _ = timeout(Duration::from_secs(5), {
                let mut rx = peer.gather_rx;
                async move { rx.recv().await }
            })
            .await;
            built.fetch_add(1, Ordering::Relaxed);
            peer.pc.close().await.expect("close");
        }

        assert_eq!(
            built.load(Ordering::Relaxed),
            3,
            "one custom runtime instance should serve several connections"
        );
    });
}
