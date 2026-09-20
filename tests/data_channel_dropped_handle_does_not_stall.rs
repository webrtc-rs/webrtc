//! Dropping one data channel's handle must not stop every *other* channel from receiving.
//!
//! `DataChannelImpl::drop` unregisters the channel's event sender, but it does not close
//! the SCTP stream, so from the driver's side a dropped channel is indistinguishable from
//! one whose `OnOpen` registration is still in flight: both simply have no sender.
//!
//! The retention added for webrtc#901 (delivering a message that arrives before its
//! channel's `OnOpen`) must therefore not treat "no sender" as "back-pressure". If it does,
//! `flush_pending_data_channel_events` reports blocked forever, `poll_reads` returns before
//! `drain_core_data` on every iteration, and *every* data channel on the connection stops
//! receiving — with SCTP driving `a_rwnd` to zero behind it.
//!
//! An `on_data_channel` handler that declines a channel it does not want is ordinary
//! application code, which is what makes this worth a test: the offerer opens "ignored" and
//! "used", the answerer keeps only "used", and a message on "used" must still arrive.

use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;

use webrtc::data_channel::{DataChannel, DataChannelEvent, RTCDataChannelInit};
use webrtc::peer_connection::{PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler};
use webrtc::peer_connection::{RTCIceGatheringState, RTCPeerConnectionState};
use webrtc::runtime::{Receiver, Sender, channel};

mod common;
use common::{block_on, runtime, timeout};

struct Handler {
    gather_tx: Sender<()>,
    connected_tx: Sender<()>,
    /// Set on the answerer only: forwards text received on the channel it keeps.
    received_tx: Option<Sender<String>>,
    runtime: Arc<dyn webrtc::runtime::Runtime>,
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
        let Some(received_tx) = self.received_tx.clone() else {
            return;
        };
        let label = dc.label().await.unwrap_or_default();

        // The regression trigger: an application that does not want this channel simply
        // drops the handle. `DataChannelImpl::drop` unregisters its sender while the
        // channel's own `OnOpen` is still on its way to `deliver_data_channel_event`.
        if label != "used" {
            drop(dc);
            return;
        }

        self.runtime.spawn(Box::pin(async move {
            while let Some(event) = dc.poll().await {
                match event {
                    DataChannelEvent::OnMessage(msg) => {
                        let text = String::from_utf8_lossy(&msg.data).to_string();
                        let _ = received_tx.try_send(text);
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
    gather_rx: Receiver<()>,
    connected_rx: Receiver<()>,
}

async fn build_peer(
    runtime: Arc<dyn webrtc::runtime::Runtime>,
    received_tx: Option<Sender<String>>,
) -> Result<Peer> {
    let (gather_tx, gather_rx) = channel::<()>(1);
    let (connected_tx, connected_rx) = channel::<()>(1);

    let pc = PeerConnectionBuilder::new()
        .with_handler(Arc::new(Handler {
            gather_tx,
            connected_tx,
            received_tx,
            runtime: runtime.clone(),
        }))
        .with_runtime(runtime)
        .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
        .build()
        .await?;

    Ok(Peer {
        pc: Box::new(pc),
        gather_rx,
        connected_rx,
    })
}

#[test]
fn test_dropped_data_channel_handle_does_not_stall_other_channels() {
    block_on(run()).unwrap();
}

async fn run() -> Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .is_test(true)
        .try_init()
        .ok();

    let runtime = runtime();
    let (received_tx, mut received_rx) = channel::<String>(8);

    let mut offerer = build_peer(runtime.clone(), None).await?;
    let mut answerer = build_peer(runtime.clone(), Some(received_tx)).await?;

    // "ignored" first: its OnOpen is what poisons the pending queue on the answerer.
    let _ignored = offerer
        .pc
        .create_data_channel("ignored", Some(RTCDataChannelInit::default()))
        .await?;
    let used = offerer
        .pc
        .create_data_channel("used", Some(RTCDataChannelInit::default()))
        .await?;

    let offer = offerer.pc.create_offer(None).await?;
    offerer.pc.set_local_description(offer).await?;
    let _ = timeout(Duration::from_secs(5), offerer.gather_rx.recv()).await;
    let offer_sdp = offerer.pc.local_description().await.expect("offer");

    answerer.pc.set_remote_description(offer_sdp).await?;
    let answer = answerer.pc.create_answer(None).await?;
    answerer.pc.set_local_description(answer).await?;
    let _ = timeout(Duration::from_secs(5), answerer.gather_rx.recv()).await;
    let answer_sdp = answerer.pc.local_description().await.expect("answer");
    offerer.pc.set_remote_description(answer_sdp).await?;

    timeout(Duration::from_secs(15), offerer.connected_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("timeout: offerer connect"))?;
    timeout(Duration::from_secs(5), answerer.connected_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("timeout: answerer connect"))?;

    // Wait for the kept channel to open, so the send below is not racing the handshake.
    let (open_tx, mut open_rx) = channel::<()>(1);
    {
        let dc = used.clone();
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
    timeout(Duration::from_secs(10), open_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("timeout: \"used\" never opened on the offerer"))?;

    used.send_text("still flowing").await?;

    let received = timeout(Duration::from_secs(10), received_rx.recv())
        .await
        .map_err(|_| {
            anyhow::anyhow!(
                "timeout: the answerer never received on \"used\" — dropping the \"ignored\" \
                 handle stalled the connection's data path"
            )
        })?;

    assert_eq!(Some("still flowing".to_string()), received);

    Ok(())
}
