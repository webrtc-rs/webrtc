//! Wrapper-level `DataChannel::id()` deferral: a locally-created in-band channel reports
//! `id() == None` until the DTLS role is resolved and the SCTP transport connects (W3C section 6.1
//! step 18 / section 6.1.1.3). Once the channel opens, `id()` must resolve to the assigned stream id
//! and stay stable; it is served synchronously from the impl's cached value without the async core
//! lock.
//!
//! This is the direct wrapper unit/integration pin for the deferral contract that the rtc
//! integration tests (`data_channel_stream_id_deferral.rs`) and the tokio-peer matrix cover
//! only transitively.
use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;

use webrtc::data_channel::{DataChannelEvent, RTCDataChannelId, RTCDataChannelInit};
use webrtc::peer_connection::{PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler};
use webrtc::peer_connection::{RTCIceGatheringState, RTCPeerConnectionState};
use webrtc::runtime::{Sender, channel};

mod common;
use common::{block_on, runtime, timeout};

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
fn test_data_channel_id_is_none_before_open_and_resolves_after() {
    block_on(run()).unwrap();
}

async fn run() -> Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .is_test(true)
        .try_init()
        .ok();

    let runtime = runtime();

    let (offer_gather_tx, mut offer_gather_rx) = channel::<()>(1);
    let (offer_conn_tx, mut offer_conn_rx) = channel::<()>(1);
    let (answer_gather_tx, mut answer_gather_rx) = channel::<()>(1);
    let (answer_conn_tx, mut answer_conn_rx) = channel::<()>(1);

    let offerer = PeerConnectionBuilder::new()
        .with_handler(Arc::new(Handler {
            gather_tx: offer_gather_tx,
            connected_tx: offer_conn_tx,
        }))
        .with_runtime(runtime.clone())
        .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
        .build()
        .await?;

    let dc = offerer
        .create_data_channel(
            "deferred-id",
            Some(RTCDataChannelInit {
                ordered: true,
                ..Default::default()
            }),
        )
        .await?;

    // Created before any SDP is exchanged -> DTLS role unresolved -> id is null.
    assert_eq!(
        dc.id().await,
        None,
        "a channel created while the DTLS role is Auto must report id() == None"
    );

    // Drive the channel to OnOpen.
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

    let offer = offerer.create_offer(None).await?;
    offerer.set_local_description(offer).await?;
    let _ = timeout(Duration::from_secs(5), offer_gather_rx.recv()).await;
    let offer_sdp = offerer.local_description().await.expect("offer");

    let answerer = PeerConnectionBuilder::new()
        .with_handler(Arc::new(Handler {
            gather_tx: answer_gather_tx,
            connected_tx: answer_conn_tx,
        }))
        .with_runtime(runtime.clone())
        .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
        .build()
        .await?;

    answerer.set_remote_description(offer_sdp).await?;
    let answer = answerer.create_answer(None).await?;
    answerer.set_local_description(answer).await?;
    let _ = timeout(Duration::from_secs(5), answer_gather_rx.recv()).await;
    let answer_sdp = answerer.local_description().await.expect("answer");
    offerer.set_remote_description(answer_sdp).await?;

    timeout(Duration::from_secs(15), offer_conn_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("timeout: offerer connect"))?;
    timeout(Duration::from_secs(5), answer_conn_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("timeout: answerer connect"))?;
    timeout(Duration::from_secs(10), open_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("timeout: data channel open"))?;

    // Once the channel opens, id() must resolve to a concrete stream id...
    let resolved: RTCDataChannelId = dc
        .id()
        .await
        .expect("a channel that has opened must have a non-null id");
    // ...and that id must carry the offerer's DTLS-role parity (RFC 8832 section 6: client even,
    // server odd). Both roles exercise the assignment; the assertion also guards against a
    // stale `None` cache.
    assert_eq!(
        dc.id().await,
        Some(resolved),
        "id() must be stable once assigned"
    );

    let _ = offerer.close().await;
    let _ = answerer.close().await;
    Ok(())
}

#[test]
fn test_negotiated_data_channel_has_id_immediately() {
    block_on(run_negotiated()).unwrap();
}

/// A negotiated channel with an explicit stream id has its id assigned up-front by the core,
/// so the wrapper's `create_data_channel` must take the `assigned_id.is_some()` branch and seed
/// `id()` without waiting for the DTLS role to resolve or SCTP to connect.
async fn run_negotiated() -> Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .is_test(true)
        .try_init()
        .ok();

    let runtime = runtime();

    let (gather_tx, _gather_rx) = channel::<()>(1);
    let (connected_tx, _connected_rx) = channel::<()>(1);

    let pc = PeerConnectionBuilder::new()
        .with_handler(Arc::new(Handler {
            gather_tx,
            connected_tx,
        }))
        .with_runtime(runtime.clone())
        .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
        .build()
        .await?;

    let id: RTCDataChannelId = 0;
    let dc = pc
        .create_data_channel(
            "negotiated-id",
            Some(RTCDataChannelInit {
                negotiated: Some(id),
                ordered: true,
                ..Default::default()
            }),
        )
        .await?;

    // Covers `peer_connection/mod.rs` `if let Some(stream_id) = assigned_id` branches:
    // the negotiated id is seeded into the wrapper synchronously at creation.
    assert_eq!(
        dc.id().await,
        Some(id),
        "a negotiated channel with an explicit id must report id() == Some(id) at creation"
    );

    let _ = pc.close().await;
    Ok(())
}
