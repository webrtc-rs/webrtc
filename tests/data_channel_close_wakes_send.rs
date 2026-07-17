//! Regression test for the blocking send back-pressure close/drop liveness hang.
//!
//! When a send-buffer limit is configured, a `DataChannel::send` that is parked in
//! back-pressure (outstanding bytes at the limit, waiting via `writable()` for the driver
//! to release SCTP-acked bytes) must be woken and must return `ErrDataChannelClosed` when
//! the `PeerConnection` is closed — because once the driver stops on the `Close` event it no
//! longer drains `outstanding_bytes` nor wakes blocked senders, and the channel is not
//! removed from the core map on close. Without the fix (a `closing` check in `writable()`'s
//! park loop + a `notify_waiters()` from `close`/`Drop`) the parked send spins on its 50 ms
//! liveness backstop forever, leaking the producing task and the outstanding send bytes.
//!
//! This test builds the connection with a small `with_data_channel_send_buffer_limit` so a
//! send parks deterministically on every runtime (a low limit — below the ~1 MiB SCTP
//! window — makes the gate engage regardless of scheduling; with an unbounded limit `send`
//! never parks and this path can't be exercised), floods with the blocking `send()`, waits
//! until outstanding has climbed into the gated band, closes the PC, and asserts the send
//! returns `ErrDataChannelClosed` promptly rather than hanging.
use anyhow::Result;
use bytes::BytesMut;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use webrtc::data_channel::{DataChannel, DataChannelEvent, RTCDataChannelInit};
use webrtc::error::Error;
use webrtc::peer_connection::{PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler};
use webrtc::peer_connection::{RTCIceGatheringState, RTCPeerConnectionState};
use webrtc::runtime::{Runtime, Sender, block_on, channel, default_runtime, sleep, timeout};

const CHUNK: usize = 1024; // 1 KB messages
// Small per-channel send-buffer limit, forced via the builder. A low limit — below the
// ~1 MiB SCTP receive window — makes the gate engage and a `send()` genuinely PARK
// deterministically on every runtime: with an unbounded (default) limit the sender never
// reaches the mark and never parks, so this test could not exercise the wake-on-close path
// it exists to guard.
const SEND_LIMIT: usize = 256 * 1024;

struct GatherHandler {
    gather_tx: Sender<()>,
    connected_tx: Sender<()>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for GatherHandler {
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

struct ReceiverHandler {
    gather_tx: Sender<()>,
    connected_tx: Sender<()>,
    received: Arc<AtomicUsize>,
    runtime: Arc<dyn Runtime>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for ReceiverHandler {
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
        let received = self.received.clone();
        self.runtime.spawn(Box::pin(async move {
            while let Some(event) = dc.poll().await {
                match event {
                    DataChannelEvent::OnMessage(msg) => {
                        received.fetch_add(msg.data.len(), Ordering::Relaxed);
                    }
                    DataChannelEvent::OnClose | DataChannelEvent::OnError => break,
                    _ => {}
                }
            }
        }));
    }
}

#[test]
fn test_close_wakes_parked_backpressured_send() {
    block_on(run()).unwrap();
}

async fn run() -> Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .is_test(true)
        .try_init()
        .ok();

    let runtime = default_runtime().ok_or_else(|| std::io::Error::other("no async runtime"))?;

    let (snd_gather_tx, mut snd_gather_rx) = channel::<()>(1);
    let (snd_conn_tx, mut snd_conn_rx) = channel::<()>(1);
    let (rcv_gather_tx, mut rcv_gather_rx) = channel::<()>(1);
    let (rcv_conn_tx, mut rcv_conn_rx) = channel::<()>(1);
    let received = Arc::new(AtomicUsize::new(0));

    // ── Sender (with a small send-buffer limit so send() parks deterministically) ─
    let sender_pc = PeerConnectionBuilder::new()
        .with_handler(Arc::new(GatherHandler {
            gather_tx: snd_gather_tx,
            connected_tx: snd_conn_tx,
        }))
        .with_runtime(runtime.clone())
        .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
        .with_data_channel_send_buffer_limit(SEND_LIMIT)
        .build()
        .await?;

    let dc = sender_pc
        .create_data_channel(
            "backpressure",
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
    let offer_sdp = sender_pc
        .local_description()
        .await
        .expect("sender local description");

    // ── Receiver ──────────────────────────────────────────────────────────────
    let receiver_pc = PeerConnectionBuilder::new()
        .with_handler(Arc::new(ReceiverHandler {
            gather_tx: rcv_gather_tx,
            connected_tx: rcv_conn_tx,
            received: received.clone(),
            runtime: runtime.clone(),
        }))
        .with_runtime(runtime.clone())
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

    // ── Flood forever with the BLOCKING send() until it errors ──────────────────
    // Signal is `true` when the terminating error was ErrDataChannelClosed (the expected
    // wake-on-close), `false` for any other error. If close() fails to wake a parked send,
    // this task never signals and the timeout below fires — the regression.
    let (done_tx, mut done_rx) = channel::<bool>(1);
    {
        let dc = dc.clone();
        runtime.spawn(Box::pin(async move {
            let buf = BytesMut::from(vec![0u8; CHUNK].as_slice());
            loop {
                if let Err(e) = dc.send(buf.clone()).await {
                    let _ = done_tx.try_send(matches!(e, Error::ErrDataChannelClosed));
                    break;
                }
            }
        }));
    }

    // Wait until the send pipeline has climbed into the gated band (within a couple of chunks
    // of the limit), so the gate is engaged and a send is parked (or one send away from it)
    // when we close — the state whose wake-on-close this test guards.
    let park_threshold = SEND_LIMIT - 2 * CHUNK;
    let mut parked = false;
    for _ in 0..200 {
        if dc.outstanding_bytes().await? >= park_threshold {
            parked = true;
            break;
        }
        sleep(Duration::from_millis(50)).await;
    }
    assert!(
        parked,
        "sender never reached the send-buffer limit; the back-pressure path was not exercised"
    );

    // Close the sender PC. A parked send() must wake and return within the timeout; without
    // the fix it spins on its 50 ms liveness backstop forever and this recv times out.
    sender_pc.close().await?;

    let was_closed_err = timeout(Duration::from_secs(5), done_rx.recv())
        .await
        .map_err(|_| {
            anyhow::anyhow!(
                "BUG: a back-pressured send() did not return after close() — liveness hang"
            )
        })?
        .ok_or_else(|| anyhow::anyhow!("flood task ended without reporting an error"))?;

    assert!(
        was_closed_err,
        "parked send() returned an error other than ErrDataChannelClosed after close()"
    );

    receiver_pc.close().await?;
    Ok(())
}
