//! A reliable, ordered data channel must deliver **every** message even when the
//! application drains it slowly.
//!
//! Regression test for [webrtc#858](https://github.com/webrtc-rs/webrtc/issues/858).
//!
//! Before the fix, the driver handed messages to the application over a bounded 256-slot
//! channel with `try_send` and, on `TrySendError::Full`, logged and dropped them: a consumer
//! that stalled long enough for the peer to push 256 messages lost data on a channel whose
//! whole contract is that it does not. This delivered 255 of 2000.
//!
//! The fix was not a bigger queue — any finite queue plus `try_send` still drops. It is two
//! halves that only work together: the driver keeps what does not fit and **stops pulling**
//! from the core (E2-02), and the core then stops draining SCTP's reassembly queues while
//! that backlog persists (E2-01), so `a_rwnd` falls and the peer is finally told to slow
//! down. Back-pressure with nowhere to go would just be a slower leak.
//!
//! The second test is the guard rail on *how* that was done: the driver must never `await`
//! the send, because the same loop drives ICE consent, DTLS retransmits and SCTP timers.
use anyhow::Result;
use bytes::BytesMut;
use rtc::peer_connection::configuration::setting_engine::SettingEngineBuilder;
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
fn test_slow_consumer_loses_nothing() {
    block_on(slow_consumer_loses_nothing()).unwrap();
}

#[test]
fn test_slow_consumer_does_not_stall_the_connection() {
    block_on(slow_consumer_does_not_stall_the_connection()).unwrap();
}

// ---------------------------------------------------------------------------------------
// Back-pressure actually reaching SCTP, and through it the sender.
//
// The test above proves the driver does not *lose* messages. It does not prove the driver
// stops pulling from the core — with an unbounded retain map it would deliver everything
// just the same, having moved the backlog into this process's heap and throttled nobody.
// That is the "memory leak with extra steps" the design warns about, and it needs the
// window to be the binding constraint to be observable at all: 2000 × 16 bytes is 32 KB,
// nowhere near SCTP's 1 MiB default, so the test above never engages SCTP flow control.
//
// Here the receive window is small and the payloads are large, so an undrained receiver
// closes the window; and the sender has a send-buffer limit, so a closed window blocks
// `send()`. The sender being blocked *is* the end-to-end proof: it means bytes stayed in
// the receiver's reassembly queue, `a_rwnd` fell, and the peer was told to slow down.
// ---------------------------------------------------------------------------------------

/// Small enough that an undrained consumer closes the window quickly, and above the RFC 4960
/// 1500-byte floor the setter clamps to.
const SMALL_RECV_WINDOW: u32 = 64 * 1024;

/// Bounds the sender's own buffer so a closed peer window surfaces as a blocked `send()`
/// rather than unbounded local queuing. Without this the default is `usize::MAX` and `send`
/// never blocks, so back-pressure would be invisible from the application.
const SEND_BUFFER_LIMIT: usize = 32 * 1024;

const BULK_PAYLOAD: usize = 4 * 1024;

/// Must exceed `DRIVER_TO_DATA_CHANNEL_EVENT_CHANNEL_CAPACITY` (256), or the driver never
/// retains anything and never stops pulling — 256 messages fit in the hand-off channel
/// untouched, so the whole chain under test stays dormant and the sender finishes freely.
/// That was the first version of this test, and it failed for that reason rather than a bug.
const BULK_MESSAGES: u32 = 600;

async fn backpressure_reaches_the_sender() -> Result<()> {
    let runtime = runtime();
    let received = Arc::new(Received::default());
    let consumer_started = Arc::new(AtomicBool::new(false));

    let (snd_gather_tx, mut snd_gather_rx) = channel::<()>(1);
    let (snd_conn_tx, mut snd_conn_rx) = channel::<()>(1);
    let (rcv_gather_tx, mut rcv_gather_rx) = channel::<()>(1);

    let sender_pc: Arc<dyn PeerConnection> = Arc::new(
        PeerConnectionBuilder::new()
            .with_handler(Arc::new(SenderHandler {
                gather_tx: snd_gather_tx,
                connected_tx: snd_conn_tx,
                watch: Arc::new(ConnectionWatch::default()),
            }))
            .with_runtime(runtime.clone())
            .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
            .with_data_channel_send_buffer_limit(SEND_BUFFER_LIMIT)
            .build()
            .await?,
    );

    let dc = sender_pc
        .create_data_channel("backpressure", Some(RTCDataChannelInit::default()))
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

    let setting_engine = SettingEngineBuilder::new()
        .with_sctp_max_receive_buffer_size(SMALL_RECV_WINDOW)
        .build();

    let receiver_pc: Arc<dyn PeerConnection> = Arc::new(
        PeerConnectionBuilder::new()
            .with_handler(Arc::new(SlowReceiverHandler {
                gather_tx: rcv_gather_tx,
                received,
                consumer_started: consumer_started.clone(),
                watch: Arc::new(ConnectionWatch::default()),
                // Long enough that the window is still shut when the assertion runs.
                stall: Duration::from_secs(10),
                runtime: runtime.clone(),
            }))
            .with_runtime(runtime.clone())
            .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
            .with_setting_engine(setting_engine)
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

    for _ in 0..100 {
        if consumer_started.load(Ordering::Acquire) {
            break;
        }
        sleep(Duration::from_millis(10)).await;
    }

    // Push 1 MiB at a receiver that is not reading, with only 64 KiB of window and 32 KiB of
    // local send buffer. If back-pressure works this cannot complete: the window shuts and
    // `send` parks. If the driver keeps draining the core into an unbounded retain map, the
    // reassembly queue empties, the window never shuts, and this races to completion.
    let sent = Arc::new(AtomicUsize::new(0));
    {
        let dc = dc.clone();
        let sent = sent.clone();
        runtime.spawn(Box::pin(async move {
            for i in 0..BULK_MESSAGES {
                let mut buf = BytesMut::from(&i.to_be_bytes()[..]);
                buf.resize(BULK_PAYLOAD, 0);
                if dc.send(buf).await.is_err() {
                    break;
                }
                sent.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }

    sleep(Duration::from_secs(3)).await;
    let sent_while_stalled = sent.load(Ordering::Relaxed);

    assert!(
        sent_while_stalled < BULK_MESSAGES as usize,
        "sender pushed all {} messages ({} KiB) at a consumer that never read a byte — \
         back-pressure never reached it. The driver is draining the core into its own \
         unbounded retain map instead of leaving bytes in SCTP's reassembly queue, so \
         `a_rwnd` never fell and the peer was never throttled.",
        BULK_MESSAGES,
        BULK_MESSAGES as usize * BULK_PAYLOAD / 1024,
    );

    sender_pc.close().await?;
    receiver_pc.close().await?;
    Ok(())
}

#[test]
fn test_backpressure_reaches_the_sender() {
    block_on(backpressure_reaches_the_sender()).unwrap();
}
