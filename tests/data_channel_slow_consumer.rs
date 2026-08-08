//! A reliable, ordered data channel must deliver **every** message even when the
//! application drains it slowly.
//!
//! This is the regression test for [webrtc#858](https://github.com/webrtc-rs/webrtc/issues/858)
//! (work plan task E0-01). Today it fails: the driver hands messages to the application
//! over a bounded 256-slot channel with `try_send`, and on `TrySendError::Full` the
//! message is logged and dropped. A consumer that stalls for longer than it takes the
//! peer to push 256 messages therefore loses data on a channel whose whole contract is
//! that it does not.
//!
//! The fix (E2) is *not* a bigger queue — any finite queue plus `try_send` still drops.
//! It is to stop draining SCTP's reassembly queue eagerly, so `a_rwnd` falls and the
//! peer throttles, and to have the driver retain-and-retry instead of discarding.
//!
//! **`#[ignore]` is temporary.** Both tests are written to fail on `master` and are
//! ignored so CI stays green until the fix lands; E2-02 removes the attributes. Run them
//! with `cargo test --test data_channel_slow_consumer -- --ignored`.
use anyhow::Result;
use bytes::BytesMut;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use webrtc::data_channel::{DataChannel, DataChannelEvent, RTCDataChannelInit};
use webrtc::peer_connection::{PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler};
use webrtc::peer_connection::{RTCIceGatheringState, RTCPeerConnectionState};
use webrtc::runtime::{Runtime, Sender, channel};

mod common;
use common::{block_on, runtime, sleep, timeout};

/// Comfortably more than the 256-slot `DRIVER_TO_DATA_CHANNEL_EVENT_CHANNEL_CAPACITY`,
/// so the overflow is structural rather than a matter of timing luck.
const MESSAGE_COUNT: u32 = 2_000;

/// How long the receiving application refuses to call `poll()` in the delivery test. It
/// only has to outlast the sender's burst; the sender pushes 2000 small messages over
/// loopback in well under this, so by the time the consumer wakes the queue has long
/// since overflowed.
const CONSUMER_STALL: Duration = Duration::from_secs(2);

/// The liveness test holds the consumer stalled for longer than ICE's consent-refresh
/// interval, so a driver that blocked on a full queue would have visibly lost the
/// connection by the time the assertion runs.
const LIVENESS_STALL: Duration = Duration::from_secs(12);

/// Each message is its own index, big-endian, padded to 16 bytes. Small enough that the
/// SCTP send buffer is never the thing that throttles the sender — we want the *receive*
/// side to be the only bottleneck — and self-describing so ordering is checkable.
fn payload(index: u32) -> BytesMut {
    let mut buf = BytesMut::from(&index.to_be_bytes()[..]);
    buf.resize(16, 0);
    buf
}

fn index_of(data: &[u8]) -> u32 {
    u32::from_be_bytes(
        data[..4]
            .try_into()
            .expect("message shorter than its index"),
    )
}

/// Records whether a peer ever *left* `Connected`. The trait has no connection-state
/// getter, so the callback is the only observation point — and "did it ever break" is
/// the stronger assertion anyway: a connection that drops and re-establishes during the
/// stall has still failed the property this test exists to protect.
#[derive(Default)]
struct ConnectionWatch {
    lost: AtomicBool,
}

impl ConnectionWatch {
    fn observe(&self, state: RTCPeerConnectionState) {
        if matches!(
            state,
            RTCPeerConnectionState::Disconnected | RTCPeerConnectionState::Failed
        ) {
            self.lost.store(true, Ordering::Release);
        }
    }
}

struct SenderHandler {
    gather_tx: Sender<()>,
    connected_tx: Sender<()>,
    watch: Arc<ConnectionWatch>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for SenderHandler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            let _ = self.gather_tx.try_send(());
        }
    }
    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        if state == RTCPeerConnectionState::Connected {
            let _ = self.connected_tx.try_send(());
        }
        self.watch.observe(state);
    }
}

/// What the receiving application observed, in arrival order.
#[derive(Default)]
struct Received {
    indices: Mutex<Vec<u32>>,
    done_tx: Mutex<Option<Sender<()>>>,
}

impl Received {
    fn record(&self, index: u32, expected: u32) {
        let complete = {
            let mut indices = self.indices.lock().unwrap();
            indices.push(index);
            indices.len() as u32 >= expected
        };
        if complete && let Some(tx) = self.done_tx.lock().unwrap().take() {
            let _ = tx.try_send(());
        }
    }
}

struct SlowReceiverHandler {
    gather_tx: Sender<()>,
    received: Arc<Received>,
    /// Set once the consumer task has been spawned, so the sender can be held back until
    /// the receiving side genuinely exists — otherwise a fast sender could finish before
    /// there is anything to stall.
    consumer_started: Arc<AtomicBool>,
    watch: Arc<ConnectionWatch>,
    stall: Duration,
    runtime: Arc<dyn Runtime>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for SlowReceiverHandler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            let _ = self.gather_tx.try_send(());
        }
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        self.watch.observe(state);
    }

    async fn on_data_channel(&self, dc: Arc<dyn DataChannel>) {
        let received = self.received.clone();
        let consumer_started = self.consumer_started.clone();
        let stall = self.stall;
        self.runtime.spawn(Box::pin(async move {
            // The slow consumer: it exists, it is subscribed, it simply is not reading
            // yet. This models an application doing real per-message work — a disk
            // write, a database round-trip — not a bug in the application.
            consumer_started.store(true, Ordering::Release);
            sleep(stall).await;

            while let Some(event) = dc.poll().await {
                match event {
                    DataChannelEvent::OnMessage(msg) => {
                        received.record(index_of(&msg.data), MESSAGE_COUNT);
                    }
                    DataChannelEvent::OnClose | DataChannelEvent::OnError => break,
                    _ => {}
                }
            }
        }));
    }
}

/// Connect two peers over loopback with one reliable ordered data channel, returning the
/// sender's channel handle once it is open. Kept separate from the assertions so the
/// handshake noise does not obscure what is being tested.
async fn connect_pair(
    stall: Duration,
    received: Arc<Received>,
    consumer_started: Arc<AtomicBool>,
    sender_watch: Arc<ConnectionWatch>,
    receiver_watch: Arc<ConnectionWatch>,
) -> Result<(
    Arc<dyn PeerConnection>,
    Arc<dyn PeerConnection>,
    Arc<dyn DataChannel>,
)> {
    let runtime = runtime();

    let (snd_gather_tx, mut snd_gather_rx) = channel::<()>(1);
    let (snd_conn_tx, mut snd_conn_rx) = channel::<()>(1);
    let (rcv_gather_tx, mut rcv_gather_rx) = channel::<()>(1);

    let sender_pc: Arc<dyn PeerConnection> = Arc::new(
        PeerConnectionBuilder::new()
            .with_handler(Arc::new(SenderHandler {
                gather_tx: snd_gather_tx,
                connected_tx: snd_conn_tx,
                watch: sender_watch,
            }))
            .with_runtime(runtime.clone())
            .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
            .build()
            .await?,
    );

    // Default init == reliable and ordered: the contract this test is about.
    let dc = sender_pc
        .create_data_channel("slow-consumer", Some(RTCDataChannelInit::default()))
        .await?;

    let (open_tx, mut open_rx) = channel::<()>(1);
    {
        let dc = dc.clone();
        runtime.spawn(Box::pin(async move {
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
    let offer_sdp = sender_pc.local_description().await.expect("offer");

    let receiver_pc: Arc<dyn PeerConnection> = Arc::new(
        PeerConnectionBuilder::new()
            .with_handler(Arc::new(SlowReceiverHandler {
                gather_tx: rcv_gather_tx,
                received,
                consumer_started,
                watch: receiver_watch,
                stall,
                runtime: runtime.clone(),
            }))
            .with_runtime(runtime.clone())
            .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
            .build()
            .await?,
    );

    receiver_pc.set_remote_description(offer_sdp).await?;
    let answer = receiver_pc.create_answer(None).await?;
    receiver_pc.set_local_description(answer).await?;
    let _ = timeout(Duration::from_secs(5), rcv_gather_rx.recv()).await;
    let answer_sdp = receiver_pc.local_description().await.expect("answer");
    sender_pc.set_remote_description(answer_sdp).await?;

    timeout(Duration::from_secs(15), snd_conn_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("timeout: peers did not connect"))?;
    timeout(Duration::from_secs(10), open_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("timeout: data channel did not open"))?;

    Ok((sender_pc, receiver_pc, dc))
}

async fn slow_consumer_loses_nothing() -> Result<()> {
    let received = Arc::new(Received::default());
    let consumer_started = Arc::new(AtomicBool::new(false));
    let (done_tx, mut done_rx) = channel::<()>(1);
    *received.done_tx.lock().unwrap() = Some(done_tx);

    let (sender_pc, receiver_pc, dc) = connect_pair(
        CONSUMER_STALL,
        received.clone(),
        consumer_started.clone(),
        Arc::new(ConnectionWatch::default()),
        Arc::new(ConnectionWatch::default()),
    )
    .await?;

    // Wait for the consumer task to be spawned (and therefore stalled) before sending, so
    // "the queue overflowed" is the reason for any loss rather than "nobody was listening".
    for _ in 0..100 {
        if consumer_started.load(Ordering::Acquire) {
            break;
        }
        sleep(Duration::from_millis(10)).await;
    }
    assert!(
        consumer_started.load(Ordering::Acquire),
        "receiver never opened the data channel"
    );

    for i in 0..MESSAGE_COUNT {
        dc.send(payload(i)).await?;
    }

    // Generous: the consumer wakes after CONSUMER_STALL and then drains as fast as it can.
    // Once the fix lands the peer will have been throttled, so the tail arrives after the
    // consumer starts reading — that is the point, and it is why this is not a tight bound.
    let _ = timeout(CONSUMER_STALL + Duration::from_secs(20), done_rx.recv()).await;

    let indices = received.indices.lock().unwrap().clone();

    let expected: Vec<u32> = (0..MESSAGE_COUNT).collect();

    // The first index that did not arrive. A truncated-but-contiguous prefix — which is
    // exactly what overflow produces — has no *mismatch*, so falling back to the length
    // is what names the real boundary rather than reporting "no gap found".
    let first_missing = indices
        .iter()
        .enumerate()
        .find(|(i, got)| **got != *i as u32)
        .map(|(i, _)| i as u32)
        .unwrap_or(indices.len() as u32);

    assert_eq!(
        indices.len(),
        MESSAGE_COUNT as usize,
        "reliable channel lost {} of {} messages while the consumer was slow \
         (delivery stops at index {}) — capacity overflow must back-pressure SCTP, not discard",
        MESSAGE_COUNT as usize - indices.len(),
        MESSAGE_COUNT,
        first_missing,
    );
    assert_eq!(
        indices, expected,
        "ordered channel delivered messages out of order"
    );

    sender_pc.close().await?;
    receiver_pc.close().await?;
    Ok(())
}

/// The connection must survive the stall: back-pressure means *the driver stops pulling*,
/// never *the driver blocks*. If a fix ever awaits the send inside the driver loop, ICE
/// consent stops being refreshed and the connection drops — this catches that regression,
/// which is the one way of "fixing" #858 that is worse than the bug.
async fn slow_consumer_does_not_stall_the_connection() -> Result<()> {
    let received = Arc::new(Received::default());
    let consumer_started = Arc::new(AtomicBool::new(false));
    let sender_watch = Arc::new(ConnectionWatch::default());
    let receiver_watch = Arc::new(ConnectionWatch::default());

    let (sender_pc, receiver_pc, dc) = connect_pair(
        LIVENESS_STALL,
        received.clone(),
        consumer_started.clone(),
        sender_watch.clone(),
        receiver_watch.clone(),
    )
    .await?;

    for _ in 0..100 {
        if consumer_started.load(Ordering::Acquire) {
            break;
        }
        sleep(Duration::from_millis(10)).await;
    }

    // Push enough to overflow while the consumer stays stalled for LIVENESS_STALL, and
    // check the connection is still up at the end of that window.
    let sent = Arc::new(AtomicUsize::new(0));
    {
        let dc = dc.clone();
        let sent = sent.clone();
        runtime().spawn(Box::pin(async move {
            for i in 0..MESSAGE_COUNT {
                if dc.send(payload(i)).await.is_err() {
                    break;
                }
                sent.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }

    sleep(LIVENESS_STALL - Duration::from_secs(2)).await;

    assert!(
        !sender_watch.lost.load(Ordering::Acquire),
        "sender left Connected while a consumer was stalled ({} messages sent) — \
         back-pressure must stop the driver pulling, not block the driver loop",
        sent.load(Ordering::Relaxed),
    );
    assert!(
        !receiver_watch.lost.load(Ordering::Acquire),
        "receiver left Connected while its own consumer was stalled — \
         ICE consent stopped being refreshed, which means the driver loop blocked"
    );

    sender_pc.close().await?;
    receiver_pc.close().await?;
    Ok(())
}

#[test]
#[ignore = "E0-01: fails until #858 is fixed in E2 (driver retains; rtc bounds the SCTP drain)"]
fn test_slow_consumer_loses_nothing() {
    block_on(slow_consumer_loses_nothing()).unwrap();
}

#[test]
#[ignore = "E0-01: guards the E2 fix against blocking the driver loop; runs with --ignored"]
fn test_slow_consumer_does_not_stall_the_connection() {
    block_on(slow_consumer_does_not_stall_the_connection()).unwrap();
}
