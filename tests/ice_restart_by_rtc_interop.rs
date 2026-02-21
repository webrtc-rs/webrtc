/// Integration test for ICE restart initiated by rtc (sansio) when communicating with webrtc
///
/// This test verifies that the rtc library can successfully initiate ICE restart
/// when communicating with the webrtc library.
use anyhow::Result;
use bytes::BytesMut;
use futures::FutureExt;
use rtc::sansio::Protocol;
use rtc::shared::{TaggedBytesMut, TransportContext, TransportProtocol};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rtc::peer_connection::RTCPeerConnectionBuilder;
use rtc::peer_connection::configuration::RTCConfigurationBuilder;
use rtc::peer_connection::configuration::setting_engine::SettingEngine;
use rtc::peer_connection::event::RTCDataChannelEvent;
use rtc::peer_connection::event::RTCPeerConnectionEvent;
use rtc::peer_connection::message::RTCMessage;
use rtc::peer_connection::state::RTCIceConnectionState;
use rtc::peer_connection::state::RTCPeerConnectionState;
use rtc::peer_connection::transport::RTCDtlsRole;
use rtc::peer_connection::transport::{CandidateConfig, CandidateHostConfig};
use webrtc::data_channel::{DataChannel, DataChannelEvent};
use webrtc::peer_connection::RTCIceGatheringState;
use webrtc::peer_connection::{PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler};
use webrtc::runtime::{Runtime, Sender, block_on, channel, default_runtime, sleep, timeout};

const DEFAULT_TIMEOUT_DURATION: Duration = Duration::from_secs(30);
const TEST_MESSAGE: &str = "Hello before restart!";
const TEST_MESSAGE_AFTER_RESTART: &str = "Hello after restart!";

struct WebrtcHandler {
    gather_complete_tx: Sender<()>,
    state_tx: Sender<RTCPeerConnectionState>,
    webrtc_msg_tx: Sender<String>,
    runtime: Arc<dyn Runtime>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for WebrtcHandler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            let _ = self.gather_complete_tx.try_send(());
        }
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        let _ = self.state_tx.try_send(state);
    }

    async fn on_data_channel(&self, dc: Arc<dyn DataChannel>) {
        let label = dc.label().await.unwrap_or_default();
        log::info!("WebRTC received data channel: {}", label);
        let msg_tx = self.webrtc_msg_tx.clone();
        self.runtime.spawn(Box::pin(async move {
            while let Some(event) = dc.poll().await {
                match event {
                    DataChannelEvent::OnOpen => log::info!("WebRTC data channel opened"),
                    DataChannelEvent::OnMessage(msg) => {
                        let s = String::from_utf8(msg.data.to_vec()).unwrap_or_default();
                        log::info!("WebRTC received: {}", s);
                        msg_tx.try_send(s).ok();
                    }
                    DataChannelEvent::OnClose => {
                        log::info!("WebRTC data channel closed");
                        break;
                    }
                    _ => {}
                }
            }
        }));
    }
}

/// Test ICE restart initiated by RTC
#[test]
fn test_ice_restart_by_rtc_interop() {
    block_on(run_test()).unwrap();
}

async fn run_test() -> Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .is_test(true)
        .try_init()
        .ok();

    log::info!("=== Starting ICE Restart Test (initiated by RTC) ===");

    let (gather_complete_tx, mut gather_complete_rx) = channel::<()>();
    let (state_tx, mut state_rx) = channel::<RTCPeerConnectionState>();
    let (webrtc_msg_tx, mut webrtc_msg_rx) = channel::<String>();

    let runtime =
        default_runtime().ok_or_else(|| std::io::Error::other("no async runtime found"))?;

    // Create RTC peer (offerer, creates DC)
    let std_socket = std::net::UdpSocket::bind("127.0.0.1:0")?;
    let rtc_local_addr = std_socket.local_addr()?;
    let rtc_socket = runtime.wrap_udp_socket(std_socket)?;
    log::info!("RTC peer bound to {}", rtc_local_addr);

    let mut setting_engine = SettingEngine::default();
    setting_engine.set_answering_dtls_role(RTCDtlsRole::Server)?;

    let config = RTCConfigurationBuilder::new().build();

    let mut rtc_pc = RTCPeerConnectionBuilder::new()
        .with_configuration(config.clone())
        .with_setting_engine(setting_engine)
        .build()?;
    log::info!("Created RTC peer connection");

    let dc_label = "test";
    let _ = rtc_pc.create_data_channel(dc_label, None)?;
    log::info!("RTC created data channel: {}", dc_label);

    let rtc_candidate = CandidateHostConfig {
        base_config: CandidateConfig {
            network: "udp".to_owned(),
            address: rtc_local_addr.ip().to_string(),
            port: rtc_local_addr.port(),
            component: 1,
            ..Default::default()
        },
        ..Default::default()
    }
    .new_candidate_host()?;
    let rtc_candidate_init =
        rtc::peer_connection::transport::RTCIceCandidate::from(&rtc_candidate).to_json()?;
    rtc_pc.add_local_candidate(rtc_candidate_init)?;

    let offer = rtc_pc.create_offer(None)?;
    rtc_pc.set_local_description(offer.clone())?;
    log::info!("RTC created offer");

    // Create WebRTC peer (answerer)
    let handler = Arc::new(WebrtcHandler {
        gather_complete_tx,
        state_tx,
        webrtc_msg_tx,
        runtime: runtime.clone(),
    });

    let webrtc_pc: Arc<dyn PeerConnection> = Arc::new(
        PeerConnectionBuilder::new()
            .with_configuration(config.clone())
            .with_handler(handler)
            .with_runtime(runtime.clone())
            .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
            .build()
            .await?,
    );
    log::info!("Created webrtc peer connection");

    webrtc_pc.set_remote_description(offer).await?;
    log::info!("WebRTC set remote description (offer)");

    let answer = webrtc_pc.create_answer(None).await?;
    webrtc_pc.set_local_description(answer.clone()).await?;
    log::info!("WebRTC created and set answer");

    let _ = timeout(Duration::from_secs(5), gather_complete_rx.recv()).await;
    let answer_with_cands = webrtc_pc
        .local_description()
        .await
        .expect("local description should be set");
    log::info!("WebRTC answer with candidates ready");

    let rtc_answer =
        rtc::peer_connection::sdp::RTCSessionDescription::answer(answer_with_cands.sdp.clone())?;
    rtc_pc.set_remote_description(rtc_answer)?;
    log::info!("RTC set remote description (answer)");

    // ──────────── Phase 1: wait for initial connection + DC open ────────────
    let mut buf = vec![0u8; 2000];
    let mut rtc_connected = false;
    let mut webrtc_connected = false;
    let mut rtc_dc_id: Option<u16> = None;
    let mut rtc_received: Vec<String> = Vec::new();

    log::info!("Waiting for initial connection...");
    let phase_start = Instant::now();

    loop {
        if phase_start.elapsed() > Duration::from_secs(15) {
            return Err(anyhow::anyhow!("Initial connection timeout"));
        }

        while let Some(msg) = rtc_pc.poll_write() {
            if let Err(err) = rtc_socket
                .send_to(&msg.message, msg.transport.peer_addr)
                .await
            {
                log::error!("Failed to send message to peer {}", err);
            }
        }

        while let Some(event) = rtc_pc.poll_event() {
            match event {
                RTCPeerConnectionEvent::OnIceConnectionStateChangeEvent(state) => {
                    log::info!("RTC ICE state: {}", state);
                    if state == RTCIceConnectionState::Failed {
                        return Err(anyhow::anyhow!("RTC ICE failed"));
                    }
                }
                RTCPeerConnectionEvent::OnConnectionStateChangeEvent(state) => {
                    log::info!("RTC peer connection state: {}", state);
                    if state == RTCPeerConnectionState::Failed {
                        return Err(anyhow::anyhow!("RTC peer connection failed"));
                    }
                    if state == RTCPeerConnectionState::Connected {
                        log::info!("RTC connected!");
                        rtc_connected = true;
                    }
                }
                RTCPeerConnectionEvent::OnDataChannel(dc_event) => {
                    if let RTCDataChannelEvent::OnOpen(channel_id) = dc_event {
                        let dc = rtc_pc.data_channel(channel_id).expect("dc should exist");
                        log::info!("RTC DC '{}' opened (id: {})", dc.label(), channel_id);
                        rtc_dc_id = Some(channel_id);
                    }
                }
                _ => {}
            }
        }

        while let Some(message) = rtc_pc.poll_read() {
            if let RTCMessage::DataChannelMessage(_, dc_msg) = message {
                let s = String::from_utf8(dc_msg.data.to_vec()).unwrap_or_default();
                rtc_received.push(s);
            }
        }

        while let Ok(state) = state_rx.try_recv() {
            if state == RTCPeerConnectionState::Connected {
                log::info!("WebRTC connected!");
                webrtc_connected = true;
            }
        }

        if rtc_connected && webrtc_connected && rtc_dc_id.is_some() {
            log::info!("Both peers connected and DC open!");
            break;
        }

        let eto = rtc_pc
            .poll_timeout()
            .unwrap_or(Instant::now() + DEFAULT_TIMEOUT_DURATION);
        let delay = eto
            .checked_duration_since(Instant::now())
            .unwrap_or_default();
        if delay.is_zero() {
            rtc_pc.handle_timeout(Instant::now())?;
            continue;
        }

        let timer = sleep(delay);
        futures::select! {
            _ = timer.fuse() => { rtc_pc.handle_timeout(Instant::now())?; }
            res = rtc_socket.recv_from(&mut buf).fuse() => {
                match res {
                    Ok((n, peer_addr)) => {
                        rtc_pc.handle_read(TaggedBytesMut {
                            now: Instant::now(),
                            transport: TransportContext {
                                local_addr: rtc_local_addr,
                                peer_addr,
                                ecn: None,
                                transport_protocol: TransportProtocol::UDP,
                            },
                            message: BytesMut::from(&buf[..n]),
                        })?;
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(e) => return Err(e.into()),
                }
            }
        }
    }

    // ──────────── Phase 2: exchange messages before ICE restart ────────────
    log::info!("Sending initial message...");

    if let Some(channel_id) = rtc_dc_id {
        let mut dc = rtc_pc.data_channel(channel_id).expect("dc should exist");
        dc.send_text(TEST_MESSAGE)?;
        log::info!("RTC sent: {}", TEST_MESSAGE);
    }

    // Flush RTC writes and wait for WebRTC to receive
    let phase_start = Instant::now();
    loop {
        if phase_start.elapsed() > Duration::from_secs(5) {
            return Err(anyhow::anyhow!(
                "Timeout waiting for TEST_MESSAGE on WebRTC"
            ));
        }
        while let Some(msg) = rtc_pc.poll_write() {
            if let Err(err) = rtc_socket
                .send_to(&msg.message, msg.transport.peer_addr)
                .await
            {
                log::error!("Failed to send message to peer {}", err);
            }
        }
        while let Some(message) = rtc_pc.poll_read() {
            if let RTCMessage::DataChannelMessage(_, dc_msg) = message {
                let s = String::from_utf8(dc_msg.data.to_vec()).unwrap_or_default();
                rtc_received.push(s);
            }
        }
        if webrtc_msg_rx
            .try_recv()
            .map_or(false, |m| m == TEST_MESSAGE)
        {
            log::info!("WebRTC received message: {}", TEST_MESSAGE);
            break;
        }
        let eto = rtc_pc
            .poll_timeout()
            .unwrap_or(Instant::now() + DEFAULT_TIMEOUT_DURATION);
        let delay = eto
            .checked_duration_since(Instant::now())
            .unwrap_or_default()
            .min(Duration::from_millis(50));
        if delay.is_zero() {
            rtc_pc.handle_timeout(Instant::now())?;
            continue;
        }
        let timer = sleep(delay);
        futures::select! {
            _ = timer.fuse() => { rtc_pc.handle_timeout(Instant::now())?; }
            res = rtc_socket.recv_from(&mut buf).fuse() => {
                match res {
                    Ok((n, peer_addr)) => {
                        rtc_pc.handle_read(TaggedBytesMut {
                            now: Instant::now(),
                            transport: TransportContext {
                                local_addr: rtc_local_addr,
                                peer_addr,
                                ecn: None,
                                transport_protocol: TransportProtocol::UDP,
                            },
                            message: BytesMut::from(&buf[..n]),
                        })?;
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(e) => return Err(e.into()),
                }
            }
        }
    }
    log::info!("Initial message received by WebRTC");

    // ──────────── Phase 3: ICE restart (initiated by RTC) ────────────
    log::info!("=== Initiating ICE restart from RTC peer ===");

    // Get old ICE credentials for verification
    let old_ufrag = rtc_pc
        .local_description()
        .and_then(|d| d.unmarshal().ok())
        .and_then(|sd| {
            sd.media_descriptions
                .first()
                .and_then(|m| m.attribute("ice-ufrag"))
                .flatten()
                .map(|s| s.to_string())
        });

    rtc_pc.restart_ice();
    let restart_offer = rtc_pc.create_offer(None)?;

    // Verify ICE credentials changed
    let new_ufrag = restart_offer.unmarshal().ok().and_then(|sd| {
        sd.media_descriptions
            .first()
            .and_then(|m| m.attribute("ice-ufrag"))
            .flatten()
            .map(|s| s.to_string())
    });
    if old_ufrag.is_some() && new_ufrag.is_some() && old_ufrag == new_ufrag {
        return Err(anyhow::anyhow!(
            "ICE ufrag did not change after restart_ice"
        ));
    }
    log::info!(
        "ICE credentials changed: ufrag {:?} -> {:?}",
        old_ufrag,
        new_ufrag
    );

    rtc_pc.set_local_description(restart_offer.clone())?;

    // Add candidate again for the restart offer
    let restart_candidate = CandidateHostConfig {
        base_config: CandidateConfig {
            network: "udp".to_owned(),
            address: rtc_local_addr.ip().to_string(),
            port: rtc_local_addr.port(),
            component: 1,
            ..Default::default()
        },
        ..Default::default()
    }
    .new_candidate_host()?;
    let restart_candidate_init =
        rtc::peer_connection::transport::RTCIceCandidate::from(&restart_candidate).to_json()?;
    rtc_pc.add_local_candidate(restart_candidate_init)?;

    let restart_offer_with_cands = rtc_pc
        .local_description()
        .expect("local description should be set");
    log::info!("RTC restart offer ready");

    // Reset state tracking
    rtc_connected = false;
    webrtc_connected = false;
    while state_rx.try_recv().is_ok() {}
    // Drain gather_complete for the next gathering
    while gather_complete_rx.try_recv().is_ok() {}

    // Feed restart offer to WebRTC
    webrtc_pc
        .set_remote_description(restart_offer_with_cands)
        .await?;
    log::info!("WebRTC set remote description for restart");

    let restart_answer = webrtc_pc.create_answer(None).await?;
    webrtc_pc
        .set_local_description(restart_answer.clone())
        .await?;
    log::info!("WebRTC created restart answer");

    let _ = timeout(Duration::from_secs(5), gather_complete_rx.recv()).await;
    let restart_answer_with_cands = webrtc_pc
        .local_description()
        .await
        .expect("local description should be set");
    log::info!("WebRTC restart answer with candidates ready");

    let rtc_restart_answer = rtc::peer_connection::sdp::RTCSessionDescription::answer(
        restart_answer_with_cands.sdp.clone(),
    )?;
    rtc_pc.set_remote_description(rtc_restart_answer)?;
    log::info!("RTC set remote description for restart (answer)");

    // ──────────── Phase 4: wait for reconnection ────────────
    log::info!("Waiting for reconnection after ICE restart...");
    let phase_start = Instant::now();
    loop {
        if phase_start.elapsed() > Duration::from_secs(15) {
            return Err(anyhow::anyhow!("ICE restart reconnection timeout"));
        }

        while let Some(msg) = rtc_pc.poll_write() {
            if let Err(err) = rtc_socket
                .send_to(&msg.message, msg.transport.peer_addr)
                .await
            {
                log::error!("Failed to send message to peer {}", err);
            }
        }
        while let Some(event) = rtc_pc.poll_event() {
            match event {
                RTCPeerConnectionEvent::OnIceConnectionStateChangeEvent(state) => {
                    log::info!("RTC ICE state (restart): {}", state);
                }
                RTCPeerConnectionEvent::OnConnectionStateChangeEvent(state) => {
                    log::info!("RTC peer connection state (restart): {}", state);
                    if state == RTCPeerConnectionState::Connected {
                        log::info!("RTC reconnected!");
                        rtc_connected = true;
                    }
                }
                _ => {}
            }
        }
        while let Some(message) = rtc_pc.poll_read() {
            if let RTCMessage::DataChannelMessage(_, dc_msg) = message {
                let s = String::from_utf8(dc_msg.data.to_vec()).unwrap_or_default();
                rtc_received.push(s);
            }
        }
        while let Ok(state) = state_rx.try_recv() {
            if state == RTCPeerConnectionState::Connected {
                log::info!("WebRTC reconnected!");
                webrtc_connected = true;
            }
        }

        if rtc_connected && webrtc_connected {
            log::info!("Both peers reconnected after ICE restart!");
            break;
        }

        let eto = rtc_pc
            .poll_timeout()
            .unwrap_or(Instant::now() + DEFAULT_TIMEOUT_DURATION);
        let delay = eto
            .checked_duration_since(Instant::now())
            .unwrap_or_default();
        if delay.is_zero() {
            rtc_pc.handle_timeout(Instant::now())?;
            continue;
        }

        let timer = sleep(delay);
        futures::select! {
            _ = timer.fuse() => { rtc_pc.handle_timeout(Instant::now())?; }
            res = rtc_socket.recv_from(&mut buf).fuse() => {
                match res {
                    Ok((n, peer_addr)) => {
                        rtc_pc.handle_read(TaggedBytesMut {
                            now: Instant::now(),
                            transport: TransportContext {
                                local_addr: rtc_local_addr,
                                peer_addr,
                                ecn: None,
                                transport_protocol: TransportProtocol::UDP,
                            },
                            message: BytesMut::from(&buf[..n]),
                        })?;
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(e) => return Err(e.into()),
                }
            }
        }
    }

    // ──────────── Phase 5: exchange message after ICE restart ────────────
    log::info!("Sending message after ICE restart...");

    if let Some(channel_id) = rtc_dc_id {
        let mut dc = rtc_pc.data_channel(channel_id).expect("dc should exist");
        dc.send_text(TEST_MESSAGE_AFTER_RESTART)?;
        log::info!("RTC sent: {}", TEST_MESSAGE_AFTER_RESTART);
    }

    let phase_start = Instant::now();
    loop {
        if phase_start.elapsed() > Duration::from_secs(5) {
            return Err(anyhow::anyhow!(
                "Timeout waiting for TEST_MESSAGE_AFTER_RESTART on WebRTC"
            ));
        }
        while let Some(msg) = rtc_pc.poll_write() {
            if let Err(err) = rtc_socket
                .send_to(&msg.message, msg.transport.peer_addr)
                .await
            {
                log::error!("Failed to send message to peer {}", err);
            }
        }
        while let Some(message) = rtc_pc.poll_read() {
            if let RTCMessage::DataChannelMessage(_, dc_msg) = message {
                let s = String::from_utf8(dc_msg.data.to_vec()).unwrap_or_default();
                rtc_received.push(s);
            }
        }
        if webrtc_msg_rx
            .try_recv()
            .map_or(false, |m| m == TEST_MESSAGE_AFTER_RESTART)
        {
            log::info!(
                "WebRTC received message after restart: {}",
                TEST_MESSAGE_AFTER_RESTART
            );
            break;
        }
        let eto = rtc_pc
            .poll_timeout()
            .unwrap_or(Instant::now() + DEFAULT_TIMEOUT_DURATION);
        let delay = eto
            .checked_duration_since(Instant::now())
            .unwrap_or_default()
            .min(Duration::from_millis(50));
        if delay.is_zero() {
            rtc_pc.handle_timeout(Instant::now())?;
            continue;
        }
        let timer = sleep(delay);
        futures::select! {
            _ = timer.fuse() => { rtc_pc.handle_timeout(Instant::now())?; }
            res = rtc_socket.recv_from(&mut buf).fuse() => {
                match res {
                    Ok((n, peer_addr)) => {
                        rtc_pc.handle_read(TaggedBytesMut {
                            now: Instant::now(),
                            transport: TransportContext {
                                local_addr: rtc_local_addr,
                                peer_addr,
                                ecn: None,
                                transport_protocol: TransportProtocol::UDP,
                            },
                            message: BytesMut::from(&buf[..n]),
                        })?;
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(e) => return Err(e.into()),
                }
            }
        }
    }

    webrtc_pc.close().await?;
    rtc_pc.close()?;

    log::info!("=== ICE Restart Test (initiated by RTC) Completed Successfully ===");
    Ok(())
}
