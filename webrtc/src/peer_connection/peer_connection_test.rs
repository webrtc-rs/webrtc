use std::sync::Arc;

use bytes::Bytes;
use interceptor::registry::Registry;
use media::Sample;
use portable_atomic::AtomicU32;
use tokio::time::Duration;
use util::vnet::net::{Net, NetConfig};
use util::vnet::router::{Router, RouterConfig};
use waitgroup::WaitGroup;

use super::*;
use crate::api::interceptor_registry::register_default_interceptors;
use crate::api::media_engine::{MediaEngine, MIME_TYPE_VP8};
use crate::api::APIBuilder;
use crate::ice_transport::ice_candidate_pair::RTCIceCandidatePair;
use crate::ice_transport::ice_server::RTCIceServer;
use crate::peer_connection::configuration::RTCConfiguration;
use crate::rtp_transceiver::rtp_codec::RTCRtpCodecCapability;
use crate::stats::StatsReportType;
use crate::track::track_local::track_local_static_rtp::TrackLocalStaticRTP;
use crate::track::track_local::track_local_static_sample::TrackLocalStaticSample;
use crate::Error;

pub(crate) async fn create_vnet_pair(
) -> Result<(RTCPeerConnection, RTCPeerConnection, Arc<Mutex<Router>>)> {
    // Create a root router
    let wan = Arc::new(Mutex::new(Router::new(RouterConfig {
        cidr: "1.2.3.0/24".to_owned(),
        ..Default::default()
    })?));

    // Create a network interface for offerer
    let offer_vnet = Arc::new(Net::new(Some(NetConfig {
        static_ips: vec!["1.2.3.4".to_owned()],
        ..Default::default()
    })));

    // Add the network interface to the router
    let nic = offer_vnet.get_nic()?;
    {
        let mut w = wan.lock().await;
        w.add_net(Arc::clone(&nic)).await?;
    }
    {
        let n = nic.lock().await;
        n.set_router(Arc::clone(&wan)).await?;
    }

    let mut offer_setting_engine = SettingEngine::default();
    offer_setting_engine.set_vnet(Some(offer_vnet));
    offer_setting_engine.set_ice_timeouts(
        Some(Duration::from_secs(1)),
        Some(Duration::from_secs(1)),
        Some(Duration::from_millis(200)),
    );

    // Create a network interface for answerer
    let answer_vnet = Arc::new(Net::new(Some(NetConfig {
        static_ips: vec!["1.2.3.5".to_owned()],
        ..Default::default()
    })));

    // Add the network interface to the router
    let nic = answer_vnet.get_nic()?;
    {
        let mut w = wan.lock().await;
        w.add_net(Arc::clone(&nic)).await?;
    }
    {
        let n = nic.lock().await;
        n.set_router(Arc::clone(&wan)).await?;
    }

    let mut answer_setting_engine = SettingEngine::default();
    answer_setting_engine.set_vnet(Some(answer_vnet));
    answer_setting_engine.set_ice_timeouts(
        Some(Duration::from_secs(1)),
        Some(Duration::from_secs(1)),
        Some(Duration::from_millis(200)),
    );

    // Start the virtual network by calling Start() on the root router
    {
        let mut w = wan.lock().await;
        w.start().await?;
    }

    let mut offer_media_engine = MediaEngine::default();
    offer_media_engine.register_default_codecs()?;
    let offer_peer_connection = APIBuilder::new()
        .with_setting_engine(offer_setting_engine)
        .with_media_engine(offer_media_engine)
        .build()
        .new_peer_connection(RTCConfiguration::default())
        .await?;

    let mut answer_media_engine = MediaEngine::default();
    answer_media_engine.register_default_codecs()?;
    let answer_peer_connection = APIBuilder::new()
        .with_setting_engine(answer_setting_engine)
        .with_media_engine(answer_media_engine)
        .build()
        .new_peer_connection(RTCConfiguration::default())
        .await?;

    Ok((offer_peer_connection, answer_peer_connection, wan))
}

/// new_pair creates two new peer connections (an offerer and an answerer)
/// *without* using an api (i.e. using the default settings).
pub(crate) async fn new_pair(api: &API) -> Result<(RTCPeerConnection, RTCPeerConnection)> {
    let pca = api.new_peer_connection(RTCConfiguration::default()).await?;
    let pcb = api.new_peer_connection(RTCConfiguration::default()).await?;

    Ok((pca, pcb))
}

pub(crate) async fn signal_pair(
    pc_offer: &mut RTCPeerConnection,
    pc_answer: &mut RTCPeerConnection,
) -> Result<()> {
    // Note(albrow): We need to create a data channel in order to trigger ICE
    // candidate gathering in the background for the JavaScript/Wasm bindings. If
    // we don't do this, the complete offer including ICE candidates will never be
    // generated.
    pc_offer
        .create_data_channel("initial_data_channel", None)
        .await?;

    let offer = pc_offer.create_offer(None).await?;

    let mut offer_gathering_complete = pc_offer.gathering_complete_promise().await;
    pc_offer.set_local_description(offer).await?;

    let _ = offer_gathering_complete.recv().await;

    pc_answer
        .set_remote_description(
            pc_offer
                .local_description()
                .await
                .ok_or(Error::new("non local description".to_owned()))?,
        )
        .await?;

    let answer = pc_answer.create_answer(None).await?;

    let mut answer_gathering_complete = pc_answer.gathering_complete_promise().await;
    pc_answer.set_local_description(answer).await?;

    let _ = answer_gathering_complete.recv().await;

    pc_offer
        .set_remote_description(
            pc_answer
                .local_description()
                .await
                .ok_or(Error::new("non local description".to_owned()))?,
        )
        .await
}

pub(crate) async fn close_pair_now(pc1: &RTCPeerConnection, pc2: &RTCPeerConnection) {
    let mut fail = false;
    if let Err(err) = pc1.close().await {
        log::error!("Failed to close PeerConnection: {}", err);
        fail = true;
    }
    if let Err(err) = pc2.close().await {
        log::error!("Failed to close PeerConnection: {}", err);
        fail = true;
    }

    assert!(!fail);
}

pub(crate) async fn close_pair(
    pc1: &RTCPeerConnection,
    pc2: &RTCPeerConnection,
    mut done_rx: mpsc::Receiver<()>,
) {
    let timeout = tokio::time::sleep(Duration::from_secs(10));
    tokio::pin!(timeout);

    tokio::select! {
        _ = timeout.as_mut() =>{
            panic!("close_pair timed out waiting for done signal");
        }
        _ = done_rx.recv() =>{
            close_pair_now(pc1, pc2).await;
        }
    }
}

/*
func offerMediaHasDirection(offer SessionDescription, kind RTPCodecType, direction RTPTransceiverDirection) bool {
    parsed := &sdp.SessionDescription{}
    if err := parsed.Unmarshal([]byte(offer.SDP)); err != nil {
        return false
    }

    for _, media := range parsed.MediaDescriptions {
        if media.MediaName.Media == kind.String() {
            _, exists := media.Attribute(direction.String())
            return exists
        }
    }
    return false
}*/

pub(crate) async fn send_video_until_done(
    mut done_rx: mpsc::Receiver<()>,
    tracks: Vec<Arc<TrackLocalStaticSample>>,
    data: Bytes,
    max_sends: Option<usize>,
) -> bool {
    let mut sends = 0;

    loop {
        let timeout = tokio::time::sleep(Duration::from_millis(20));
        tokio::pin!(timeout);

        tokio::select! {
            biased;

            _ = done_rx.recv() =>{
                log::debug!("sendVideoUntilDone received done");
                return false;
            }

            _ = timeout.as_mut() =>{
                if max_sends.map(|s| sends >= s).unwrap_or(false) {
                    continue;
                }

                log::debug!("sendVideoUntilDone timeout");
                for track in &tracks {
                    log::debug!("sendVideoUntilDone track.WriteSample");
                    let result = track.write_sample(&Sample{
                        data: data.clone(),
                        duration: Duration::from_secs(1),
                        ..Default::default()
                    }).await;
                    assert!(result.is_ok());
                    sends += 1;
                }
            }
        }
    }
}

pub(crate) async fn until_connection_state(
    pc: &mut RTCPeerConnection,
    wg: &WaitGroup,
    state: RTCPeerConnectionState,
) {
    let w = Arc::new(Mutex::new(Some(wg.worker())));
    pc.on_peer_connection_state_change(Box::new(move |pcs: RTCPeerConnectionState| {
        let w2 = Arc::clone(&w);
        Box::pin(async move {
            if pcs == state {
                let mut worker = w2.lock().await;
                worker.take();
            }
        })
    }));
}

#[tokio::test]
async fn test_get_stats() -> Result<()> {
    let mut m = MediaEngine::default();
    m.register_default_codecs()?;
    let api = APIBuilder::new().with_media_engine(m).build();

    let (mut pc_offer, mut pc_answer) = new_pair(&api).await?;

    let (ice_complete_tx, mut ice_complete_rx) = mpsc::channel::<()>(1);
    let ice_complete_tx = Arc::new(Mutex::new(Some(ice_complete_tx)));
    pc_answer.on_ice_connection_state_change(Box::new(move |ice_state: RTCIceConnectionState| {
        let ice_complete_tx2 = Arc::clone(&ice_complete_tx);
        Box::pin(async move {
            if ice_state == RTCIceConnectionState::Connected {
                tokio::time::sleep(Duration::from_secs(1)).await;
                let mut done = ice_complete_tx2.lock().await;
                done.take();
            }
        })
    }));

    let sender_called_candidate_change = Arc::new(AtomicU32::new(0));
    let sender_called_candidate_change2 = Arc::clone(&sender_called_candidate_change);
    pc_offer
        .sctp()
        .transport()
        .ice_transport()
        .on_selected_candidate_pair_change(Box::new(move |_: RTCIceCandidatePair| {
            sender_called_candidate_change2.store(1, Ordering::SeqCst);
            Box::pin(async {})
        }));
    let track = Arc::new(TrackLocalStaticSample::new(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_VP8.to_owned(),
            ..Default::default()
        },
        "video".to_owned(),
        "webrtc-rs".to_owned(),
    ));
    pc_offer
        .add_track(track.clone())
        .await
        .expect("Failed to add track");
    let (packet_tx, packet_rx) = mpsc::channel(1);

    pc_answer.on_track(Box::new(move |track, _, _| {
        let packet_tx = packet_tx.clone();
        tokio::spawn(async move {
            while let Ok((pkt, _)) = track.read_rtp().await {
                dbg!(&pkt);
                let last = pkt.payload[pkt.payload.len() - 1];

                if last == 0xAA {
                    let _ = packet_tx.send(()).await;
                    break;
                }
            }
        });

        Box::pin(async move {})
    }));

    signal_pair(&mut pc_offer, &mut pc_answer).await?;

    let _ = ice_complete_rx.recv().await;
    send_video_until_done(
        packet_rx,
        vec![track],
        Bytes::from_static(b"\xDE\xAD\xBE\xEF\xAA"),
        Some(1),
    )
    .await;

    let offer_stats = pc_offer.get_stats().await;
    assert!(!offer_stats.reports.is_empty());

    match offer_stats.reports.get("ice_transport") {
        Some(StatsReportType::Transport(ice_transport_stats)) => {
            assert!(ice_transport_stats.bytes_received > 0);
            assert!(ice_transport_stats.bytes_sent > 0);
        }
        Some(_other) => panic!("found the wrong type"),
        None => panic!("missed it"),
    }
    let outbound_stats = offer_stats
        .reports
        .values()
        .find_map(|v| match v {
            StatsReportType::OutboundRTP(d) => Some(d),
            _ => None,
        })
        .expect("Should have produced an RTP Outbound stat");
    assert_eq!(outbound_stats.packets_sent, 1);
    assert_eq!(outbound_stats.kind, "video");
    assert_eq!(outbound_stats.bytes_sent, 8);
    assert_eq!(outbound_stats.header_bytes_sent, 12);

    let answer_stats = pc_answer.get_stats().await;
    let inbound_stats = answer_stats
        .reports
        .values()
        .find_map(|v| match v {
            StatsReportType::InboundRTP(d) => Some(d),
            _ => None,
        })
        .expect("Should have produced an RTP inbound stat");
    assert_eq!(inbound_stats.packets_received, 1);
    assert_eq!(inbound_stats.kind, "video");
    assert_eq!(inbound_stats.bytes_received, 8);
    assert_eq!(inbound_stats.header_bytes_received, 12);

    close_pair_now(&pc_offer, &pc_answer).await;

    Ok(())
}

#[tokio::test]
async fn test_peer_connection_close_is_send() -> Result<()> {
    let handle = tokio::spawn(async move { peer().await });
    tokio::join!(handle).0.unwrap()
}

#[tokio::test]
async fn test_set_get_configuration() {
    // initialize MediaEngine and InterceptorRegistry
    let media_engine = MediaEngine::default();
    let registry = Registry::default();

    // create API instance
    let api = APIBuilder::new()
        .with_media_engine(media_engine)
        .with_interceptor_registry(registry)
        .build();

    // create configuration
    let initial_config = RTCConfiguration {
        ice_servers: vec![RTCIceServer {
            urls: vec!["stun:stun.l.google.com:19302".to_string()],
            username: "".to_string(),
            credential: "".to_string(),
        }],
        ..Default::default()
    };

    // create RTCPeerConnection instance
    let peer = Arc::new(
        api.new_peer_connection(initial_config.clone())
            .await
            .expect("Failed to create RTCPeerConnection"),
    );

    // get configuration and println
    let config_before = peer.get_configuration().await;
    println!("Initial ICE Servers: {:?}", config_before.ice_servers);
    println!(
        "Initial ICE Transport Policy: {:?}",
        config_before.ice_transport_policy
    );
    println!("Initial Bundle Policy: {:?}", config_before.bundle_policy);
    println!(
        "Initial RTCP Mux Policy: {:?}",
        config_before.rtcp_mux_policy
    );
    println!("Initial Peer Identity: {:?}", config_before.peer_identity);
    println!("Initial Certificates: {:?}", config_before.certificates);
    println!(
        "Initial ICE Candidate Pool Size: {:?}",
        config_before.ice_candidate_pool_size
    );

    // create new configuration
    let new_config = RTCConfiguration {
        ice_servers: vec![RTCIceServer {
            urls: vec![
                "turn:turn.22333.fun".to_string(),
                "turn:cn.22333.fun".to_string(),
            ],
            username: "live777".to_string(),
            credential: "live777".to_string(),
        }],
        ..Default::default()
    };

    // set new configuration
    peer.set_configuration(new_config.clone())
        .await
        .expect("Failed to set configuration");

    // get new configuration and println
    let updated_config = peer.get_configuration().await;
    println!("Updated ICE Servers: {:?}", updated_config.ice_servers);
    println!(
        "Updated ICE Transport Policy: {:?}",
        updated_config.ice_transport_policy
    );
    println!("Updated Bundle Policy: {:?}", updated_config.bundle_policy);
    println!(
        "Updated RTCP Mux Policy: {:?}",
        updated_config.rtcp_mux_policy
    );
    println!("Updated Peer Identity: {:?}", updated_config.peer_identity);
    println!("Updated Certificates: {:?}", updated_config.certificates);
    println!(
        "Updated ICE Candidate Pool Size: {:?}",
        updated_config.ice_candidate_pool_size
    );

    // verify
    assert_eq!(updated_config.ice_servers, new_config.ice_servers);
}

async fn peer() -> Result<()> {
    let mut m = MediaEngine::default();
    m.register_default_codecs()?;
    let mut registry = Registry::new();
    registry = register_default_interceptors(registry, &mut m)?;
    let api = APIBuilder::new()
        .with_media_engine(m)
        .with_interceptor_registry(registry)
        .build();

    let config = RTCConfiguration {
        ice_servers: vec![RTCIceServer {
            urls: vec!["stun:stun.l.google.com:19302".to_owned()],
            ..Default::default()
        }],
        ..Default::default()
    };

    let peer_connection = Arc::new(api.new_peer_connection(config).await?);

    let offer = peer_connection.create_offer(None).await?;
    let mut gather_complete = peer_connection.gathering_complete_promise().await;
    peer_connection.set_local_description(offer).await?;
    let _ = gather_complete.recv().await;

    if peer_connection.local_description().await.is_some() {
        //TODO?
    }

    peer_connection.close().await?;

    Ok(())
}

pub(crate) fn on_connected() -> (OnPeerConnectionStateChangeHdlrFn, mpsc::Receiver<()>) {
    let (done_tx, done_rx) = mpsc::channel::<()>(1);
    let done_tx = Arc::new(Mutex::new(Some(done_tx)));
    let hdlr_fn: OnPeerConnectionStateChangeHdlrFn =
        Box::new(move |state: RTCPeerConnectionState| {
            let done_tx_clone = Arc::clone(&done_tx);
            Box::pin(async move {
                if state == RTCPeerConnectionState::Connected {
                    let mut tx = done_tx_clone.lock().await;
                    tx.take();
                }
            })
        });
    (hdlr_fn, done_rx)
}

// Everytime we receive a new SSRC we probe it and try to determine the proper way to handle it.
// In most cases a Track explicitly declares a SSRC and a OnTrack is fired. In two cases we don't
// know the SSRC ahead of time
// * Undeclared SSRC in a single media section
// * Simulcast
//
// The Undeclared SSRC processing code would run before Simulcast. If a Simulcast Offer/Answer only
// contained one Media Section we would never fire the OnTrack. We would assume it was a failed
// Undeclared SSRC processing. This test asserts that we properly handled this.
#[tokio::test]
async fn test_peer_connection_simulcast_no_data_channel() -> Result<()> {
    let mut m = MediaEngine::default();
    for ext in [
        ::sdp::extmap::SDES_MID_URI,
        ::sdp::extmap::SDES_RTP_STREAM_ID_URI,
    ] {
        m.register_header_extension(
            RTCRtpHeaderExtensionCapability {
                uri: ext.to_owned(),
            },
            RTPCodecType::Video,
            None,
        )?;
    }
    m.register_default_codecs()?;
    let api = APIBuilder::new().with_media_engine(m).build();

    let (mut pc_send, mut pc_recv) = new_pair(&api).await?;
    let (send_notifier, mut send_connected) = on_connected();
    let (recv_notifier, mut recv_connected) = on_connected();
    pc_send.on_peer_connection_state_change(send_notifier);
    pc_recv.on_peer_connection_state_change(recv_notifier);
    let (track_tx, mut track_rx) = mpsc::unbounded_channel();
    pc_recv.on_track(Box::new(move |t, _, _| {
        let rid = t.rid().to_owned();
        let _ = track_tx.send(rid);
        Box::pin(async move {})
    }));

    let id = "video";
    let stream_id = "webrtc-rs";
    let track = Arc::new(TrackLocalStaticRTP::new_with_rid(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_VP8.to_owned(),
            ..Default::default()
        },
        id.to_owned(),
        "a".to_owned(),
        stream_id.to_owned(),
    ));
    let track_a = Arc::clone(&track);
    let transceiver = pc_send.add_transceiver_from_track(track, None).await?;
    let sender = transceiver.sender().await;

    let track = Arc::new(TrackLocalStaticRTP::new_with_rid(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_VP8.to_owned(),
            ..Default::default()
        },
        id.to_owned(),
        "b".to_owned(),
        stream_id.to_owned(),
    ));
    let track_b = Arc::clone(&track);
    sender.add_encoding(track).await?;

    let track = Arc::new(TrackLocalStaticRTP::new_with_rid(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_VP8.to_owned(),
            ..Default::default()
        },
        id.to_owned(),
        "c".to_owned(),
        stream_id.to_owned(),
    ));
    let track_c = Arc::clone(&track);
    sender.add_encoding(track).await?;

    // signaling
    signal_pair(&mut pc_send, &mut pc_recv).await?;
    let _ = send_connected.recv().await;
    let _ = recv_connected.recv().await;

    for sequence_number in [0; 100] {
        let pkt = rtp::packet::Packet {
            header: rtp::header::Header {
                version: 2,
                sequence_number,
                payload_type: 96,
                ..Default::default()
            },
            payload: Bytes::from_static(&[0; 2]),
        };

        track_a.write_rtp_with_extensions(&pkt, &[]).await?;
        track_b.write_rtp_with_extensions(&pkt, &[]).await?;
        track_c.write_rtp_with_extensions(&pkt, &[]).await?;
    }

    assert_eq!(track_rx.recv().await.unwrap(), "a".to_owned());
    assert_eq!(track_rx.recv().await.unwrap(), "b".to_owned());
    assert_eq!(track_rx.recv().await.unwrap(), "c".to_owned());

    close_pair_now(&pc_send, &pc_recv).await;

    Ok(())
}

#[tokio::test]
async fn test_peer_connection_state() -> Result<()> {
    let mut m = MediaEngine::default();
    m.register_default_codecs()?;
    let api = APIBuilder::new().with_media_engine(m).build();
    let pc = api.new_peer_connection(RTCConfiguration::default()).await?;

    assert_eq!(pc.connection_state(), RTCPeerConnectionState::New);

    RTCPeerConnection::update_connection_state(
        &pc.internal.on_peer_connection_state_change_handler,
        &pc.internal.is_closed,
        &pc.internal.peer_connection_state,
        RTCIceConnectionState::Checking,
        RTCDtlsTransportState::New,
    )
    .await;
    assert_eq!(pc.connection_state(), RTCPeerConnectionState::Connecting);

    RTCPeerConnection::update_connection_state(
        &pc.internal.on_peer_connection_state_change_handler,
        &pc.internal.is_closed,
        &pc.internal.peer_connection_state,
        RTCIceConnectionState::Connected,
        RTCDtlsTransportState::New,
    )
    .await;
    assert_eq!(pc.connection_state(), RTCPeerConnectionState::Connecting);

    RTCPeerConnection::update_connection_state(
        &pc.internal.on_peer_connection_state_change_handler,
        &pc.internal.is_closed,
        &pc.internal.peer_connection_state,
        RTCIceConnectionState::Connected,
        RTCDtlsTransportState::Connecting,
    )
    .await;
    assert_eq!(pc.connection_state(), RTCPeerConnectionState::Connecting);

    RTCPeerConnection::update_connection_state(
        &pc.internal.on_peer_connection_state_change_handler,
        &pc.internal.is_closed,
        &pc.internal.peer_connection_state,
        RTCIceConnectionState::Connected,
        RTCDtlsTransportState::Connected,
    )
    .await;
    assert_eq!(pc.connection_state(), RTCPeerConnectionState::Connected);

    RTCPeerConnection::update_connection_state(
        &pc.internal.on_peer_connection_state_change_handler,
        &pc.internal.is_closed,
        &pc.internal.peer_connection_state,
        RTCIceConnectionState::Completed,
        RTCDtlsTransportState::Connected,
    )
    .await;
    assert_eq!(pc.connection_state(), RTCPeerConnectionState::Connected);

    RTCPeerConnection::update_connection_state(
        &pc.internal.on_peer_connection_state_change_handler,
        &pc.internal.is_closed,
        &pc.internal.peer_connection_state,
        RTCIceConnectionState::Connected,
        RTCDtlsTransportState::Closed,
    )
    .await;
    assert_eq!(pc.connection_state(), RTCPeerConnectionState::Connected);

    RTCPeerConnection::update_connection_state(
        &pc.internal.on_peer_connection_state_change_handler,
        &pc.internal.is_closed,
        &pc.internal.peer_connection_state,
        RTCIceConnectionState::Disconnected,
        RTCDtlsTransportState::Connected,
    )
    .await;
    assert_eq!(pc.connection_state(), RTCPeerConnectionState::Disconnected);

    RTCPeerConnection::update_connection_state(
        &pc.internal.on_peer_connection_state_change_handler,
        &pc.internal.is_closed,
        &pc.internal.peer_connection_state,
        RTCIceConnectionState::Failed,
        RTCDtlsTransportState::Connected,
    )
    .await;
    assert_eq!(pc.connection_state(), RTCPeerConnectionState::Failed);

    RTCPeerConnection::update_connection_state(
        &pc.internal.on_peer_connection_state_change_handler,
        &pc.internal.is_closed,
        &pc.internal.peer_connection_state,
        RTCIceConnectionState::Connected,
        RTCDtlsTransportState::Failed,
    )
    .await;
    assert_eq!(pc.connection_state(), RTCPeerConnectionState::Failed);

    pc.close().await?;
    assert_eq!(pc.connection_state(), RTCPeerConnectionState::Closed);

    Ok(())
}
