/// Integration test for data channel close behavior between rtc and webrtc
///
/// This test verifies that:
/// - RTC can create a data channel (as offerer)
/// - RTC can send periodic messages to WebRTC
/// - RTC can close the data channel after sending N messages
/// - WebRTC properly detects the data channel close event
///
/// This mirrors the data-channels-close example behavior where RTC (answerer in the example)
/// manages the data channel lifecycle.
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
use rtc::peer_connection::state::RTCIceConnectionState;
use rtc::peer_connection::state::RTCPeerConnectionState;
use rtc::peer_connection::transport::RTCDtlsRole;
use rtc::peer_connection::transport::{CandidateConfig, CandidateHostConfig};
use webrtc::data_channel::{DataChannel, DataChannelEvent};
use webrtc::peer_connection::{PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler};
use webrtc::peer_connection::{RTCIceGatheringState, RTCPeerConnectionState as WebrtcPCState};
use webrtc::runtime::{Runtime, Sender, block_on, channel, default_runtime, sleep, timeout};

const DEFAULT_TIMEOUT_DURATION: Duration = Duration::from_secs(30);

struct WebrtcHandler {
    gather_complete_tx: Sender<()>,
    connected_tx: Sender<()>,
    webrtc_msg_tx: Sender<String>,
    webrtc_closed_tx: Sender<()>,
    runtime: Arc<dyn Runtime>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for WebrtcHandler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            let _ = self.gather_complete_tx.try_send(());
        }
    }

    async fn on_connection_state_change(&self, state: WebrtcPCState) {
        if state == WebrtcPCState::Connected {
            let _ = self.connected_tx.try_send(());
        }
    }

    async fn on_data_channel(&self, dc: Arc<dyn DataChannel>) {
        let label = dc.label().await.unwrap_or_default();
        log::info!("WebRTC received data channel: {}", label);
        let webrtc_msg_tx = self.webrtc_msg_tx.clone();
        let webrtc_closed_tx = self.webrtc_closed_tx.clone();
        self.runtime.spawn(Box::pin(async move {
            while let Some(event) = dc.poll().await {
                match event {
                    DataChannelEvent::OnOpen => {
                        log::info!("WebRTC data channel opened");
                    }
                    DataChannelEvent::OnMessage(msg) => {
                        let data = String::from_utf8(msg.data.to_vec()).unwrap_or_default();
                        log::info!("WebRTC received message: '{}'", data);
                        webrtc_msg_tx.try_send(data).ok();
                    }
                    DataChannelEvent::OnClose => {
                        log::info!("WebRTC data channel closed");
                        webrtc_closed_tx.try_send(()).ok();
                        break;
                    }
                    _ => {}
                }
            }
        }));
    }
}

/// Test data channels close behavior with RTC sending periodic messages and closing
#[test]
fn test_data_channels_close_interop_by_rtc_interop() {
    block_on(run_test()).unwrap();
}

async fn run_test() -> Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .is_test(true)
        .try_init()
        .ok();

    log::info!("Starting data channel close interop test: RTC sends and closes");

    let (gather_complete_tx, mut gather_complete_rx) = channel::<()>();
    let (connected_tx, mut connected_rx) = channel::<()>();
    let (webrtc_msg_tx, mut webrtc_msg_rx) = channel::<String>();
    let (webrtc_closed_tx, mut webrtc_closed_rx) = channel::<()>();

    let runtime =
        default_runtime().ok_or_else(|| std::io::Error::other("no async runtime found"))?;

    let mut messages_to_send = 3usize;

    // Create rtc peer (offerer)
    let std_socket = std::net::UdpSocket::bind("127.0.0.1:0")?;
    let local_addr = std_socket.local_addr()?;
    let socket = runtime.wrap_udp_socket(std_socket)?;
    log::info!("RTC peer bound to {}", local_addr);

    let mut setting_engine = SettingEngine::default();
    setting_engine.set_answering_dtls_role(RTCDtlsRole::Server)?;

    let config = RTCConfigurationBuilder::new().build();

    let mut rtc_pc = RTCPeerConnectionBuilder::new()
        .with_configuration(config.clone())
        .with_setting_engine(setting_engine)
        .build()?;
    log::info!("Created RTC peer connection");

    let dc_label = "test-channel";
    let _rtc_dc = rtc_pc.create_data_channel(dc_label, None)?;
    log::info!("RTC created data channel: {}", dc_label);

    let candidate = CandidateHostConfig {
        base_config: CandidateConfig {
            network: "udp".to_owned(),
            address: local_addr.ip().to_string(),
            port: local_addr.port(),
            component: 1,
            ..Default::default()
        },
        ..Default::default()
    }
    .new_candidate_host()?;
    let local_candidate_init =
        rtc::peer_connection::transport::RTCIceCandidate::from(&candidate).to_json()?;
    rtc_pc.add_local_candidate(local_candidate_init)?;

    let offer = rtc_pc.create_offer(None)?;
    log::info!("RTC created offer");
    rtc_pc.set_local_description(offer.clone())?;
    log::info!("RTC set local description");

    // Create webrtc peer (answerer)
    let handler = Arc::new(WebrtcHandler {
        gather_complete_tx,
        connected_tx,
        webrtc_msg_tx,
        webrtc_closed_tx,
        runtime: runtime.clone(),
    });

    let webrtc_pc = PeerConnectionBuilder::new()
        .with_configuration(config)
        .with_handler(handler)
        .with_runtime(runtime.clone())
        .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
        .build()
        .await?;
    log::info!("Created webrtc peer connection");

    webrtc_pc.set_remote_description(offer).await?;
    log::info!("WebRTC set remote description");

    let answer = webrtc_pc.create_answer(None).await?;
    log::info!("WebRTC created answer");
    webrtc_pc.set_local_description(answer.clone()).await?;
    log::info!("WebRTC set local description");

    let _ = timeout(Duration::from_secs(5), gather_complete_rx.recv()).await;

    let answer_with_candidates = webrtc_pc
        .local_description()
        .await
        .expect("local description should be set");
    log::info!("WebRTC answer with candidates ready");

    let rtc_answer = rtc::peer_connection::sdp::RTCSessionDescription::answer(
        answer_with_candidates.sdp.clone(),
    )?;
    rtc_pc.set_remote_description(rtc_answer)?;
    log::info!("RTC set remote description");

    // Run event loop
    let mut buf = vec![0u8; 2000];
    let mut rtc_connected = false;
    let mut webrtc_connected = false;
    let mut rtc_data_channel_opened = false;
    let mut rtc_dc_id: Option<u16> = None;
    let mut last_message_time = Instant::now();
    let message_interval = Duration::from_millis(500);
    let mut webrtc_msg_count = 0usize;
    let mut webrtc_closed = false;

    let start_time = Instant::now();
    let test_timeout = Duration::from_secs(30);

    while start_time.elapsed() < test_timeout {
        // Process rtc writes
        while let Some(msg) = rtc_pc.poll_write() {
            match socket.send_to(&msg.message, msg.transport.peer_addr).await {
                Ok(n) => log::trace!("RTC sent {} bytes to {}", n, msg.transport.peer_addr),
                Err(err) => log::error!("RTC socket write error: {}", err),
            }
        }

        // Process rtc events
        while let Some(event) = rtc_pc.poll_event() {
            match event {
                RTCPeerConnectionEvent::OnIceConnectionStateChangeEvent(state) => {
                    log::info!("RTC ICE connection state: {}", state);
                    if state == RTCIceConnectionState::Failed {
                        return Err(anyhow::anyhow!("RTC ICE connection failed"));
                    }
                }
                RTCPeerConnectionEvent::OnConnectionStateChangeEvent(state) => {
                    log::info!("RTC peer connection state: {}", state);
                    if state == RTCPeerConnectionState::Failed {
                        return Err(anyhow::anyhow!("RTC peer connection failed"));
                    }
                    if state == RTCPeerConnectionState::Connected {
                        log::info!("RTC peer connection connected!");
                        rtc_connected = true;
                    }
                }
                RTCPeerConnectionEvent::OnDataChannel(dc_event) => match dc_event {
                    RTCDataChannelEvent::OnOpen(channel_id) => {
                        let dc = rtc_pc.data_channel(channel_id).expect("dc should exist");
                        log::info!(
                            "RTC data channel opened: {} (id: {})",
                            dc.label(),
                            channel_id
                        );
                        rtc_data_channel_opened = true;
                        rtc_dc_id = Some(channel_id);
                        last_message_time = Instant::now();
                    }
                    RTCDataChannelEvent::OnClose(channel_id) => {
                        log::info!("RTC data channel {} closed", channel_id);
                        rtc_data_channel_opened = false;
                    }
                    _ => {}
                },
                _ => {}
            }
        }

        // Check for webrtc connected signal
        if !webrtc_connected && connected_rx.try_recv().is_ok() {
            log::info!("WebRTC peer connection connected!");
            webrtc_connected = true;
        }

        // Drain webrtc message and close signals
        while let Ok(msg) = webrtc_msg_rx.try_recv() {
            log::info!("WebRTC collected message: '{}'", msg);
            webrtc_msg_count += 1;
        }
        if !webrtc_closed && webrtc_closed_rx.try_recv().is_ok() {
            webrtc_closed = true;
        }

        // Send periodic messages from RTC side, then close
        if rtc_connected && webrtc_connected && rtc_data_channel_opened {
            let elapsed = Instant::now().duration_since(last_message_time);
            if elapsed >= message_interval {
                if let Some(dc_id) = rtc_dc_id {
                    let mut dc = rtc_pc.data_channel(dc_id).expect("dc should exist");
                    if messages_to_send > 0 {
                        let message = format!("Message #{}", 4 - messages_to_send);
                        log::info!("RTC sending: '{}'", message);
                        dc.send_text(message)?;
                        last_message_time = Instant::now();
                        messages_to_send -= 1;
                    } else {
                        log::info!("RTC finished sending, closing data channel");
                        dc.close()?;
                    }
                }
            }
        }

        // Check if test is complete
        if webrtc_closed {
            log::info!("✅ Test completed successfully!");
            log::info!(
                "   WebRTC received {} messages before close",
                webrtc_msg_count
            );
            assert!(
                webrtc_msg_count >= 3,
                "WebRTC should have received at least 3 messages before close"
            );
            webrtc_pc.close().await?;
            rtc_pc.close()?;
            return Ok(());
        }

        // Poll timeout
        let eto = rtc_pc
            .poll_timeout()
            .unwrap_or(Instant::now() + DEFAULT_TIMEOUT_DURATION);

        let delay_from_now = eto
            .checked_duration_since(Instant::now())
            .unwrap_or(Duration::from_secs(0));
        if delay_from_now.is_zero() {
            rtc_pc.handle_timeout(Instant::now())?;
            continue;
        }

        let timer = sleep(delay_from_now);

        futures::select! {
            _ = timer.fuse() => {
                rtc_pc.handle_timeout(Instant::now())?;
            }
            res = socket.recv_from(&mut buf).fuse() => {
                match res {
                    Ok((n, peer_addr)) => {
                        log::trace!("RTC received {} bytes from {}", n, peer_addr);
                        rtc_pc.handle_read(TaggedBytesMut {
                            now: Instant::now(),
                            transport: TransportContext {
                                local_addr,
                                peer_addr,
                                ecn: None,
                                transport_protocol: TransportProtocol::UDP,
                            },
                            message: BytesMut::from(&buf[..n]),
                        })?;
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(err) => {
                        log::error!("RTC socket read error: {}", err);
                        return Err(err.into());
                    }
                }
            }
        }
    }

    webrtc_pc.close().await?;
    rtc_pc.close()?;
    Err(anyhow::anyhow!(
        "Test timeout - data channel close not detected by WebRTC in time"
    ))
}
