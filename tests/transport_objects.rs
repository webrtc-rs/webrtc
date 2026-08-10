//! The W3C transport objects are reachable from the async API, and their identities behave.
//!
//! This is the test that would have caught the original defect behind #841: every member below
//! was implemented inside `rtc`, and none of it could be called. So it asserts **real values**
//! against a live connection rather than "did not panic" — an accessor wired to the wrong field
//! would still return *something*.
//!
//! It also pins the two properties the id design rests on:
//!
//! * the DTLS transport reached through `pc.sctp()` is the same one a sender sends over, and
//! * ids are stable across walks, so `==` means "the same transport" rather than "asked at the
//!   same moment".

use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;
use webrtc::data_channel::{DataChannelEvent, RTCDataChannelInit};
use webrtc::media_stream::track_local::static_rtp::TrackLocalStaticRTP;
use webrtc::peer_connection::transport::{DtlsTransport, IceTransport, SctpTransport};
use webrtc::peer_connection::{
    MediaEngine, PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler,
};
use webrtc::peer_connection::{RTCIceGatheringState, RTCPeerConnectionState};
use webrtc::runtime::{Sender, channel};

use rtc::media_stream::MediaStreamTrack;
use rtc::peer_connection::configuration::media_engine::MIME_TYPE_VP8;
use rtc::peer_connection::state::RTCIceGatheringState as CoreGatheringState;
use rtc::peer_connection::transport::{
    RTCDtlsTransportState, RTCIceComponent, RTCIceRole, RTCIceTransportState, RTCSctpTransportState,
};
use rtc::rtp_transceiver::rtp_sender::{
    RTCRtpCodec, RTCRtpCodecParameters, RTCRtpCodingParameters, RTCRtpEncodingParameters,
    RtpCodecKind,
};

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

fn vp8() -> RTCRtpCodecParameters {
    RTCRtpCodecParameters {
        rtp_codec: RTCRtpCodec {
            mime_type: MIME_TYPE_VP8.to_owned(),
            clock_rate: 90000,
            channels: 0,
            sdp_fmtp_line: String::new(),
            rtcp_feedback: vec![],
        },
        payload_type: 96,
        ..Default::default()
    }
}

fn video_track() -> Arc<TrackLocalStaticRTP> {
    Arc::new(TrackLocalStaticRTP::new(MediaStreamTrack::new(
        "stream".to_string(),
        "video".to_string(),
        "video".to_string(),
        RtpCodecKind::Video,
        vec![RTCRtpEncodingParameters {
            rtp_coding_parameters: RTCRtpCodingParameters {
                ssrc: Some(0x1234_5678),
                ..Default::default()
            },
            codec: vp8().rtp_codec.clone(),
            ..Default::default()
        }],
    )))
}

fn video_media_engine() -> MediaEngine {
    let mut media_engine = MediaEngine::default();
    media_engine
        .register_codec(vp8(), RtpCodecKind::Video)
        .expect("register codec");
    media_engine
}

async fn transport_objects_are_reachable() -> Result<()> {
    let runtime = runtime();

    let (a_gather_tx, mut a_gather_rx) = channel::<()>(1);
    let (a_conn_tx, mut a_conn_rx) = channel::<()>(1);
    let (b_gather_tx, mut b_gather_rx) = channel::<()>(1);
    let (b_conn_tx, mut _b_conn_rx) = channel::<()>(1);

    let pc_a = PeerConnectionBuilder::new()
        .with_handler(Arc::new(Handler {
            gather_tx: a_gather_tx,
            connected_tx: a_conn_tx,
        }))
        .with_runtime(runtime.clone())
        .with_media_engine(video_media_engine())
        .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
        .build()
        .await?;

    // Both a data channel and a media track, so the SCTP and RTP halves of the graph both exist.
    let dc = pc_a
        .create_data_channel("transport-objects", Some(RTCDataChannelInit::default()))
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
    let sender = pc_a.add_track(video_track()).await?;

    // Before negotiation there is no SCTP transport, and the sender is unassociated.
    assert!(
        pc_a.sctp().await.is_none(),
        "SCTP has not been negotiated yet"
    );
    assert!(
        sender.transport().await?.is_none(),
        "an unassociated sender has a null transport"
    );

    let offer = pc_a.create_offer(None).await?;
    pc_a.set_local_description(offer).await?;
    let _ = timeout(Duration::from_secs(5), a_gather_rx.recv()).await;
    let offer_sdp = pc_a.local_description().await.expect("offer");

    let pc_b = PeerConnectionBuilder::new()
        .with_handler(Arc::new(Handler {
            gather_tx: b_gather_tx,
            connected_tx: b_conn_tx,
        }))
        .with_runtime(runtime.clone())
        .with_media_engine(video_media_engine())
        .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
        .build()
        .await?;

    pc_b.set_remote_description(offer_sdp).await?;
    let answer = pc_b.create_answer(None).await?;
    pc_b.set_local_description(answer).await?;
    let _ = timeout(Duration::from_secs(5), b_gather_rx.recv()).await;
    let answer_sdp = pc_b.local_description().await.expect("answer");
    pc_a.set_remote_description(answer_sdp).await?;

    timeout(Duration::from_secs(15), a_conn_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("timeout waiting for the connection"))?;
    // The peer-connection state covers ICE and DTLS; SCTP reports `Connected` once its own
    // association is up, which is what opening a data channel waits for.
    timeout(Duration::from_secs(10), open_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("timeout waiting for the data channel to open"))?;

    // ---- the graph is walkable ----
    let sctp: Arc<dyn SctpTransport> = pc_a.sctp().await.expect("SCTP is negotiated");
    let dtls: Arc<dyn DtlsTransport> = sctp.transport();
    let ice: Arc<dyn IceTransport> = dtls.ice_transport();

    // ---- every member returns a real value ----
    assert_eq!(RTCSctpTransportState::Connected, sctp.state().await?);
    assert!(
        sctp.max_message_size().await? > 0,
        "a negotiated association reports a usable message size"
    );
    assert!(
        sctp.max_channels().await?.is_some(),
        "maxChannels is known once the association is connected"
    );

    assert_eq!(RTCDtlsTransportState::Connected, dtls.state().await?);
    assert!(
        !dtls.get_remote_certificates().await?.is_empty(),
        "the peer's certificate is retained after the handshake"
    );

    assert_eq!(RTCIceComponent::Rtp, ice.component());
    assert_ne!(RTCIceRole::Unspecified, ice.role().await?);
    assert!(matches!(
        ice.state().await?,
        RTCIceTransportState::Connected | RTCIceTransportState::Completed
    ));
    assert_eq!(CoreGatheringState::Complete, ice.gathering_state().await?);
    assert!(!ice.get_local_candidates().await?.is_empty());
    assert!(!ice.get_remote_candidates().await?.is_empty());

    let pair = ice
        .get_selected_candidate_pair()
        .await?
        .expect("ICE nominated a pair");
    // `RTCIceCandidatePair` had no accessors at all before this work — a caller handed one could
    // not read either candidate out of it.
    assert!(!pair.local().address.is_empty());
    assert!(!pair.remote().address.is_empty());

    let local_params = ice
        .get_local_parameters()
        .await?
        .expect("local parameters after setLocalDescription");
    assert!(!local_params.username_fragment.is_empty());
    let remote_params = ice
        .get_remote_parameters()
        .await?
        .expect("remote parameters after setRemoteDescription");
    assert!(!remote_params.username_fragment.is_empty());
    assert_ne!(
        local_params.username_fragment, remote_params.username_fragment,
        "the two ends have different ufrags, so these are not the same read twice"
    );

    // ---- identity ----
    let sender_dtls = sender
        .transport()
        .await?
        .expect("an associated sender has a transport");
    assert_eq!(
        dtls.id(),
        sender_dtls.id(),
        "the sender sends over the transport the data channels use"
    );
    assert_eq!(
        ice.id(),
        sender_dtls.ice_transport().id(),
        "...and reaches the same ICE transport"
    );

    // Ids are stored, not minted per call.
    let sctp_again = pc_a.sctp().await.expect("SCTP is negotiated");
    assert_eq!(sctp.id(), sctp_again.id());
    assert_eq!(dtls.id(), sctp_again.transport().id());

    // Three transports, three identities.
    assert_ne!(sctp.id(), dtls.id());
    assert_ne!(dtls.id(), ice.id());
    assert_ne!(sctp.id(), ice.id());

    // A different connection's transports are never equal to this one's.
    let other_dtls = pc_b
        .sctp()
        .await
        .expect("peer also negotiated SCTP")
        .transport();
    assert_ne!(
        dtls.id(),
        other_dtls.id(),
        "transports of two peer connections must not compare equal"
    );

    pc_a.close().await?;
    pc_b.close().await?;
    Ok(())
}

/// A media-only connection has no SCTP transport, so a sender's `transport()` is the *only* way
/// into the DTLS and ICE objects.
///
/// This is what makes the handle's route matter: resolving through `pc.sctp()` would work on a
/// connection that happens to have a data channel and fail here.
async fn media_only_connection_reaches_dtls_through_the_sender() -> Result<()> {
    let runtime = runtime();

    let (a_gather_tx, mut a_gather_rx) = channel::<()>(1);
    let (a_conn_tx, mut a_conn_rx) = channel::<()>(1);
    let (b_gather_tx, mut b_gather_rx) = channel::<()>(1);
    let (b_conn_tx, mut _b_conn_rx) = channel::<()>(1);

    let pc_a = PeerConnectionBuilder::new()
        .with_handler(Arc::new(Handler {
            gather_tx: a_gather_tx,
            connected_tx: a_conn_tx,
        }))
        .with_runtime(runtime.clone())
        .with_media_engine(video_media_engine())
        .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
        .build()
        .await?;

    // A track and no data channel.
    let sender = pc_a.add_track(video_track()).await?;

    let offer = pc_a.create_offer(None).await?;
    pc_a.set_local_description(offer).await?;
    let _ = timeout(Duration::from_secs(5), a_gather_rx.recv()).await;
    let offer_sdp = pc_a.local_description().await.expect("offer");

    let pc_b = PeerConnectionBuilder::new()
        .with_handler(Arc::new(Handler {
            gather_tx: b_gather_tx,
            connected_tx: b_conn_tx,
        }))
        .with_runtime(runtime.clone())
        .with_media_engine(video_media_engine())
        .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
        .build()
        .await?;

    pc_b.set_remote_description(offer_sdp).await?;
    let answer = pc_b.create_answer(None).await?;
    pc_b.set_local_description(answer).await?;
    let _ = timeout(Duration::from_secs(5), b_gather_rx.recv()).await;
    let answer_sdp = pc_b.local_description().await.expect("answer");
    pc_a.set_remote_description(answer_sdp).await?;

    timeout(Duration::from_secs(15), a_conn_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("timeout waiting for the connection"))?;

    assert!(
        pc_a.sctp().await.is_none(),
        "no data channel was negotiated, so there is no SCTP transport"
    );

    let dtls = sender
        .transport()
        .await?
        .expect("an associated sender has a transport");
    assert_eq!(
        RTCDtlsTransportState::Connected,
        dtls.state().await?,
        "the sender's route reaches a live DTLS transport"
    );

    let ice = dtls.ice_transport();
    assert!(matches!(
        ice.state().await?,
        RTCIceTransportState::Connected | RTCIceTransportState::Completed
    ));
    assert!(!ice.get_local_candidates().await?.is_empty());
    assert_ne!(dtls.id(), ice.id());

    pc_a.close().await?;
    pc_b.close().await?;
    Ok(())
}

#[test]
fn test_media_only_connection_reaches_dtls_through_the_sender() {
    block_on(media_only_connection_reaches_dtls_through_the_sender()).unwrap();
}

#[test]
fn test_transport_objects_are_reachable() {
    block_on(transport_objects_are_reachable()).unwrap();
}
