//! Regression test for the non-blocking send-cap **terminal-error-on-close** property.
//!
//! After `PeerConnection::close()` the driver stops draining each channel's
//! `outstanding_bytes`, and the core does not remove the channel from its map — so if a
//! channel's outstanding bytes are pinned at the send-buffer limit (the steady state of a
//! naive flood), a `send()` that only checked the capacity gate would return the *retryable*
//! `ErrSendBufferFull` forever, and an application retrying on it (the documented pattern)
//! would livelock as a leaked task. `send()`/`send_text()` therefore check the `closing`
//! flag first and fail *terminally* with `ErrDataChannelClosed`.
//!
//! This is the non-blocking analog of the old (deleted) close-wakes park-hang test.
//!
//! The test floods until the cap engages (`ErrSendBufferFull` observed ⇒ outstanding pinned
//! at the limit), closes the connection, then asserts a subsequent `send()` and `send_text()`
//! return `ErrDataChannelClosed` — NOT `ErrSendBufferFull` (which a retry loop would spin on)
//! and NOT `Ok`. Deleting the `closing` guard makes this FAIL (it returns `ErrSendBufferFull`
//! with outstanding pinned, or `Ok` if the buffer happened to drain).
use anyhow::Result;
use bytes::BytesMut;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use webrtc::data_channel::{DataChannel, DataChannelEvent, RTCDataChannelInit};
use webrtc::error::Error;
use webrtc::peer_connection::{PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler};
use webrtc::peer_connection::{RTCIceGatheringState, RTCPeerConnectionState};
use webrtc::runtime::{Runtime, Sender, block_on, channel, default_runtime, timeout};

const CHUNK: usize = 1024;
// Tiny send-buffer limit so a short naive flood deterministically fills it and the cap
// engages (a send returns ErrSendBufferFull), pinning outstanding at the limit.
const SEND_LIMIT: usize = 64 * 1024;

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
fn test_data_channel_send_after_close_is_terminal() {
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
            "after-close",
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

    // Flood until the cap engages: a send returning ErrSendBufferFull proves outstanding is
    // pinned at SEND_LIMIT — the exact state in which, post-close, a capacity-only send()
    // would keep returning ErrSendBufferFull forever.
    let buf = BytesMut::from(vec![0u8; CHUNK].as_slice());
    let mut hit_full = false;
    for _ in 0..200_000 {
        match dc.send(buf.clone()).await {
            Ok(()) => {}
            Err(Error::ErrSendBufferFull) => {
                hit_full = true;
                break;
            }
            Err(e) => return Err(anyhow::anyhow!("unexpected send error before close: {e:?}")),
        }
    }
    assert!(
        hit_full,
        "cap never engaged during the pre-close flood — cannot exercise the pinned-buffer \
         terminal-close path"
    );

    // Close the sender. The driver stops draining outstanding_bytes; the channel stays in
    // the core map, so a capacity-only gate would still see outstanding pinned at the limit.
    sender_pc.close().await?;

    // A send()/send_text() after close must fail TERMINALLY with ErrDataChannelClosed — not
    // the retryable ErrSendBufferFull (which a retry loop would spin on forever) and not Ok.
    match dc.send(buf.clone()).await {
        Err(Error::ErrDataChannelClosed) => {}
        Err(Error::ErrSendBufferFull) => panic!(
            "send() after close returned the retryable ErrSendBufferFull (outstanding is \
             pinned and never drains post-close ⇒ a retry loop livelocks); expected the \
             terminal ErrDataChannelClosed"
        ),
        other => panic!("send() after close returned {other:?}; expected ErrDataChannelClosed"),
    }
    match dc.send_text("x").await {
        Err(Error::ErrDataChannelClosed) => {}
        other => {
            panic!("send_text() after close returned {other:?}; expected ErrDataChannelClosed")
        }
    }

    receiver_pc.close().await?;
    Ok(())
}
