/// Integration test for data channel close behavior where WebRTC closes the channel
///
/// This test verifies that:
/// - RTC can create a data channel (as offerer)
/// - WebRTC can send periodic messages to RTC
/// - WebRTC can close the data channel after sending N messages
/// - RTC properly detects the data channel close event
///
/// This is the inverse of the data_channels_close_interop test where RTC closes the channel.
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
use webrtc::peer_connection::{PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler};
use webrtc::runtime::{Runtime, Sender, block_on, channel, default_runtime, sleep, timeout};
use webrtc::{RTCIceGatheringState, RTCPeerConnectionState as WebrtcPCState};

const DEFAULT_TIMEOUT_DURATION: Duration = Duration::from_secs(30);

struct WebrtcHandler {
    gather_complete_tx: Sender<()>,
    connected_tx: Sender<()>,
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
        self.runtime.spawn(Box::pin(async move {
            let mut messages_to_send = 3usize;
            while let Some(event) = dc.poll().await {
                match event {
                    DataChannelEvent::OnOpen => {
                        log::info!(
                            "WebRTC data channel opened, will send {} messages",
                            messages_to_send
                        );
                        while messages_to_send > 0 {
                            sleep(Duration::from_millis(500)).await;
                            let message = format!("Message #{}", 4 - messages_to_send);
                            log::info!("WebRTC sending: '{}'", message);
                            if dc.send_text(&message).await.is_err() {
                                break;
                            }
                            messages_to_send -= 1;
                        }
                        // Give the last message time to be delivered before closing
                        sleep(Duration::from_millis(300)).await;
                        log::info!("WebRTC finished sending, closing data channel");
                        let _ = dc.close().await;
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

/// Test data channel close behavior with WebRTC sending periodic messages and closing
#[test]
fn test_data_channel_close_by_webrtc_interop() {
    block_on(run_test()).unwrap();
}

async fn run_test() -> Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        //.is_test(true)
        .try_init()
        .ok();

    log::info!("Starting data channel close interop test: WebRTC sends and closes");

    let local_addr_str = format!("{}:0", "127.0.0.1"); //signal::get_local_ip());

    let (gather_complete_tx, mut gather_complete_rx) = channel::<()>();
    let (connected_tx, mut connected_rx) = channel::<()>();

    let runtime =
        default_runtime().ok_or_else(|| std::io::Error::other("no async runtime found"))?;

    // Create rtc peer (offerer, creates the data channel)
    let std_socket = std::net::UdpSocket::bind(local_addr_str.as_str())?;
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
        runtime: runtime.clone(),
    });

    let webrtc_pc = PeerConnectionBuilder::new()
        .with_configuration(config)
        .with_handler(handler)
        .with_runtime(runtime.clone())
        .with_udp_addrs(vec![local_addr_str.clone()])
        .build()
        .await?;
    log::info!(
        "Created webrtc peer connection with binding to {}",
        local_addr_str
    );

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
    let mut _rtc_connected = false;
    let mut webrtc_connected = false;
    let mut _rtc_data_channel_opened = false;
    let mut rtc_channel_closed = false;
    let mut rtc_received_messages = Vec::<String>::new();

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
                        _rtc_connected = true;
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
                        _rtc_data_channel_opened = true;
                    }
                    RTCDataChannelEvent::OnClose(channel_id) => {
                        log::info!("RTC data channel {} closed", channel_id);
                        _rtc_data_channel_opened = false;
                        rtc_channel_closed = true;
                    }
                    _ => {}
                },
                _ => {}
            }
        }

        // Receive data channel messages on RTC side
        while let Some(message) = rtc_pc.poll_read() {
            if let RTCMessage::DataChannelMessage(channel_id, dc_msg) = message {
                let data = String::from_utf8(dc_msg.data.to_vec()).unwrap_or_default();
                log::info!("RTC received message on channel {}: '{}'", channel_id, data);
                rtc_received_messages.push(data);
            }
        }

        // Check for webrtc connected signal
        if !webrtc_connected && connected_rx.try_recv().is_ok() {
            log::info!("WebRTC peer connection connected!");
            webrtc_connected = true;
        }

        // Check if test is complete
        if rtc_channel_closed {
            log::info!("✅ Test completed successfully!");
            log::info!(
                "   RTC received {} messages before close: {:?}",
                rtc_received_messages.len(),
                rtc_received_messages
            );
            assert!(
                rtc_received_messages.len() >= 3,
                "RTC should have received at least 3 messages before close"
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
        "Test timeout - data channel close not detected by RTC in time"
    ))
}
