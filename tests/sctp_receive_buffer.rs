//! The `with_sctp_receive_buffer_size` builder knob plumbs the SCTP receive
//! window (a_rwnd) end-to-end, and a sub-RFC-4960-floor value is clamped up so it
//! doesn't silently break the handshake.
//!
//! Both tests establish a real data channel and deliver a message, which only
//! succeeds if the SCTP association actually formed with the configured window —
//! exercising the setter, the builder forwarders, and the `start()` branch that
//! chains `TransportConfig::with_max_receive_buffer_size`.
use anyhow::Result;
use bytes::BytesMut;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use webrtc::data_channel::{DataChannel, DataChannelEvent, RTCDataChannelInit};
use webrtc::peer_connection::{PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler};
use webrtc::peer_connection::{RTCIceGatheringState, RTCPeerConnectionState};
use webrtc::runtime::{Runtime, Sender, block_on, channel, default_runtime, sleep, timeout};

struct SenderHandler {
    gather_tx: Sender<()>,
    connected_tx: Sender<()>,
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
    }
}

struct ReceiverHandler {
    gather_tx: Sender<()>,
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

/// Build a sender+receiver pair, both with the given SCTP receive window, open a
/// data channel and deliver `msg`; assert it arrives (i.e. the association formed).
async fn exchange_one_message(recv_buf: u32, msg: &[u8]) -> Result<()> {
    let runtime = default_runtime().ok_or_else(|| std::io::Error::other("no async runtime"))?;

    let (snd_gather_tx, mut snd_gather_rx) = channel::<()>(1);
    let (snd_conn_tx, mut snd_conn_rx) = channel::<()>(1);
    let (rcv_gather_tx, mut rcv_gather_rx) = channel::<()>(1);
    let received = Arc::new(AtomicUsize::new(0));

    let sender_pc = PeerConnectionBuilder::new()
        .with_handler(Arc::new(SenderHandler {
            gather_tx: snd_gather_tx,
            connected_tx: snd_conn_tx,
        }))
        .with_runtime(runtime.clone())
        .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
        .with_sctp_receive_buffer_size(recv_buf)
        .build()
        .await?;

    let dc = sender_pc
        .create_data_channel("rwnd", Some(RTCDataChannelInit::default()))
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

    let receiver_pc = PeerConnectionBuilder::new()
        .with_handler(Arc::new(ReceiverHandler {
            gather_tx: rcv_gather_tx,
            received: received.clone(),
            runtime: runtime.clone(),
        }))
        .with_runtime(runtime.clone())
        .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
        .with_sctp_receive_buffer_size(recv_buf)
        .build()
        .await?;

    receiver_pc.set_remote_description(offer_sdp).await?;
    let answer = receiver_pc.create_answer(None).await?;
    receiver_pc.set_local_description(answer).await?;
    let _ = timeout(Duration::from_secs(5), rcv_gather_rx.recv()).await;
    let answer_sdp = receiver_pc.local_description().await.expect("answer");
    sender_pc.set_remote_description(answer_sdp).await?;

    timeout(Duration::from_secs(15), snd_conn_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("timeout: connect (recv_buf={recv_buf})"))?;
    timeout(Duration::from_secs(10), open_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("timeout: data channel open"))?;

    dc.send(BytesMut::from(msg)).await?;

    let mut delivered = false;
    for _ in 0..100 {
        if received.load(Ordering::Relaxed) >= msg.len() {
            delivered = true;
            break;
        }
        sleep(Duration::from_millis(50)).await;
    }
    assert!(
        delivered,
        "message not delivered with sctp_receive_buffer_size={recv_buf} — association did not form/flow"
    );

    sender_pc.close().await?;
    receiver_pc.close().await?;
    Ok(())
}

#[test]
fn test_sctp_receive_buffer_size_lowered() {
    // A realistic lowered window (256 KiB): the whole plumbing path + start() Some-branch.
    block_on(exchange_one_message(256 * 1024, b"hello rwnd")).unwrap();
}

#[test]
fn test_sctp_receive_buffer_size_below_rfc_floor_is_clamped() {
    // 1000 < RFC 4960's 1500-byte floor: the setter clamps it up, so the handshake
    // still succeeds and data flows. Without the clamp the peer would reject our INIT
    // (ErrInitAdvertisedReceiver1500) and this would time out.
    block_on(exchange_one_message(1000, b"clamped")).unwrap();
}
