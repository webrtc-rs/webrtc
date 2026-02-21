/// Integration test for ICE restart between rtc (sansio) and webrtc
///
/// This test verifies that the rtc library can successfully handle ICE restart
/// when communicating with the webrtc library.
use anyhow::Result;
use bytes::BytesMut;
use futures::FutureExt;
use rtc::peer_connection::configuration::RTCOfferOptions;
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
use webrtc::RTCIceGatheringState;
use webrtc::data_channel::DataChannelEvent;
use webrtc::peer_connection::{PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler};
use webrtc::runtime::{Sender, block_on, channel, default_runtime, sleep, timeout};

const DEFAULT_TIMEOUT_DURATION: Duration = Duration::from_secs(30);
const TEST_MESSAGE_1: &str = "Hello before restart!";
const ECHO_MESSAGE_1: &str = "Echo before restart!";
const TEST_MESSAGE_2: &str = "Hello after restart!";
const ECHO_MESSAGE_2: &str = "Echo after restart!";

struct WebrtcHandler {
    gather_complete_tx: Sender<()>,
    state_tx: Sender<RTCPeerConnectionState>,
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
}

/// Test ICE restart between webrtc (offerer) and rtc (answerer)
#[test]
fn test_ice_restart_by_webrtc_interop() {
    block_on(run_test()).unwrap();
}

async fn run_test() -> Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .is_test(true)
        .try_init()
        .ok();

    log::info!("Starting ICE restart interop test: webrtc <-> rtc");

    let (gather_complete_tx, mut gather_complete_rx) = channel::<()>();
    let (state_tx, mut state_rx) = channel::<RTCPeerConnectionState>();

    let runtime =
        default_runtime().ok_or_else(|| std::io::Error::other("no async runtime found"))?;

    // Create webrtc peer (offerer)
    let config = RTCConfigurationBuilder::new().build();

    let handler = Arc::new(WebrtcHandler {
        gather_complete_tx,
        state_tx,
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

    // Create data channel on webrtc side (offerer)
    let dc_label = "test-channel";
    let webrtc_dc = webrtc_pc.create_data_channel(dc_label, None).await?;
    log::info!("Created webrtc data channel: {}", dc_label);

    // Spawn DC poll task for webrtc side
    let (webrtc_msg_tx, mut webrtc_msg_rx) = channel::<String>();
    {
        let dc = webrtc_dc.clone();
        let msg_tx = webrtc_msg_tx.clone();
        runtime.spawn(Box::pin(async move {
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

    // Create initial offer from webrtc
    let webrtc_offer = webrtc_pc.create_offer(None).await?;
    webrtc_pc
        .set_local_description(webrtc_offer.clone())
        .await?;
    let _ = timeout(Duration::from_secs(5), gather_complete_rx.recv()).await;
    let offer_with_cands = webrtc_pc
        .local_description()
        .await
        .expect("local description should be set");
    log::info!("WebRTC offer with candidates ready");

    // Create RTC peer (answerer)
    let std_socket = std::net::UdpSocket::bind("127.0.0.1:0")?;
    let rtc_local_addr = std_socket.local_addr()?;
    let rtc_socket = runtime.wrap_udp_socket(std_socket)?;
    log::info!("RTC peer bound to {}", rtc_local_addr);

    let mut setting_engine = SettingEngine::default();
    setting_engine.set_answering_dtls_role(RTCDtlsRole::Client)?;

    let mut rtc_pc = RTCPeerConnectionBuilder::new()
        .with_configuration(config.clone())
        .with_setting_engine(setting_engine)
        .build()?;
    log::info!("Created RTC peer connection");

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

    let rtc_offer =
        rtc::peer_connection::sdp::RTCSessionDescription::offer(offer_with_cands.sdp.clone())?;
    rtc_pc.set_remote_description(rtc_offer)?;
    log::info!("RTC set remote description (offer)");

    let rtc_answer = rtc_pc.create_answer(None)?;
    rtc_pc.set_local_description(rtc_answer.clone())?;
    log::info!("RTC created and set answer");

    webrtc_pc.set_remote_description(rtc_answer).await?;
    log::info!("WebRTC set remote description (answer)");

    // ──────────── Phase 1: wait for initial connection ────────────
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
                        log::info!(
                            "RTC data channel '{}' opened (id: {})",
                            dc.label(),
                            channel_id
                        );
                        rtc_dc_id = Some(channel_id);
                    }
                }
                _ => {}
            }
        }

        while let Some(message) = rtc_pc.poll_read() {
            if let RTCMessage::DataChannelMessage(_, dc_msg) = message {
                let s = String::from_utf8(dc_msg.data.to_vec()).unwrap_or_default();
                log::info!("RTC received: {}", s);
                rtc_received.push(s);
            }
        }

        // Drain webrtc state changes
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
    log::info!("Testing data channel before ICE restart...");

    // WebRTC → RTC
    webrtc_dc.send_text(TEST_MESSAGE_1).await?;
    log::info!("WebRTC sent: {}", TEST_MESSAGE_1);

    let phase_start = Instant::now();
    loop {
        if phase_start.elapsed() > Duration::from_secs(5) {
            return Err(anyhow::anyhow!("Timeout waiting for TEST_MESSAGE_1 on RTC"));
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
                log::info!("RTC received: {}", s);
                rtc_received.push(s);
            }
        }
        if rtc_received.contains(&TEST_MESSAGE_1.to_string()) {
            break;
        }
        let eto = rtc_pc
            .poll_timeout()
            .unwrap_or(Instant::now() + DEFAULT_TIMEOUT_DURATION);
        let delay = eto
            .checked_duration_since(Instant::now())
            .unwrap_or_default()
            .min(Duration::from_millis(50));
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

    // RTC → WebRTC
    if let Some(channel_id) = rtc_dc_id {
        let mut dc = rtc_pc.data_channel(channel_id).expect("dc should exist");
        dc.send_text(ECHO_MESSAGE_1)?;
        log::info!("RTC sent: {}", ECHO_MESSAGE_1);
    }
    // Flush RTC writes
    let phase_start = Instant::now();
    loop {
        if phase_start.elapsed() > Duration::from_secs(5) {
            return Err(anyhow::anyhow!(
                "Timeout waiting for ECHO_MESSAGE_1 on WebRTC"
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
            .map_or(false, |m| m == ECHO_MESSAGE_1)
        {
            log::info!("WebRTC received echo: {}", ECHO_MESSAGE_1);
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

    assert!(
        rtc_received.contains(&TEST_MESSAGE_1.to_string()),
        "RTC should have received test message before restart"
    );
    log::info!("Messages exchanged successfully before ICE restart");

    // ──────────── Phase 3: ICE restart (initiated by WebRTC) ────────────
    log::info!("=== Initiating ICE restart from WebRTC ===");
    rtc_connected = false;
    webrtc_connected = false;
    // Drain stale state signals
    while state_rx.try_recv().is_ok() {}

    let restart_offer = webrtc_pc
        .create_offer(Some(RTCOfferOptions { ice_restart: true }))
        .await?;
    webrtc_pc
        .set_local_description(restart_offer.clone())
        .await?;
    let _ = timeout(Duration::from_secs(5), gather_complete_rx.recv()).await;
    let restart_offer_with_cands = webrtc_pc
        .local_description()
        .await
        .expect("local description should be set after restart offer");
    log::info!("WebRTC restart offer with candidates ready");

    // Feed new offer to RTC
    let rtc_restart_offer = rtc::peer_connection::sdp::RTCSessionDescription::offer(
        restart_offer_with_cands.sdp.clone(),
    )?;
    rtc_pc.set_remote_description(rtc_restart_offer)?;
    log::info!("RTC set remote description for restart");

    let rtc_restart_answer = rtc_pc.create_answer(None)?;
    rtc_pc.set_local_description(rtc_restart_answer.clone())?;
    log::info!("RTC created restart answer");

    webrtc_pc.set_remote_description(rtc_restart_answer).await?;
    log::info!("WebRTC set remote description for restart");

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

    // ──────────── Phase 5: exchange messages after ICE restart ────────────
    log::info!("Testing data channel after ICE restart...");

    // WebRTC → RTC
    webrtc_dc.send_text(TEST_MESSAGE_2).await?;
    log::info!("WebRTC sent after restart: {}", TEST_MESSAGE_2);

    let phase_start = Instant::now();
    loop {
        if phase_start.elapsed() > Duration::from_secs(5) {
            return Err(anyhow::anyhow!("Timeout waiting for TEST_MESSAGE_2 on RTC"));
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
                log::info!("RTC received after restart: {}", s);
                rtc_received.push(s);
            }
        }
        if rtc_received.contains(&TEST_MESSAGE_2.to_string()) {
            break;
        }
        let eto = rtc_pc
            .poll_timeout()
            .unwrap_or(Instant::now() + DEFAULT_TIMEOUT_DURATION);
        let delay = eto
            .checked_duration_since(Instant::now())
            .unwrap_or_default()
            .min(Duration::from_millis(50));
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

    // RTC → WebRTC
    if let Some(channel_id) = rtc_dc_id {
        let mut dc = rtc_pc.data_channel(channel_id).expect("dc should exist");
        dc.send_text(ECHO_MESSAGE_2)?;
        log::info!("RTC sent after restart: {}", ECHO_MESSAGE_2);
    }
    let phase_start = Instant::now();
    loop {
        if phase_start.elapsed() > Duration::from_secs(5) {
            return Err(anyhow::anyhow!(
                "Timeout waiting for ECHO_MESSAGE_2 on WebRTC"
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
            .map_or(false, |m| m == ECHO_MESSAGE_2)
        {
            log::info!("WebRTC received echo after restart: {}", ECHO_MESSAGE_2);
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

    assert!(
        rtc_received.contains(&TEST_MESSAGE_2.to_string()),
        "RTC should have received test message after restart"
    );

    webrtc_pc.close().await?;
    rtc_pc.close()?;

    log::info!("=== ICE Restart Test (initiated by WebRTC) Completed Successfully ===");
    Ok(())
}
