/// Integration tests for ICE-TCP multiplexing: several peer connections share one TCP
/// listener, dispatched by the ufrag in each accepted stream's first STUN frame.
///
/// The headline property: two connections on the **same fixed TCP port** coexist, which is
/// impossible with per-connection listener binds (`with_tcp_addrs` on one port fails the
/// second bind with `EADDRINUSE`).
use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;

use rtc::ice::network_type::NetworkType;
use rtc::peer_connection::configuration::setting_engine::SettingEngineBuilder;
use webrtc::data_channel::{DataChannel, DataChannelEvent};
use webrtc::peer_connection::transport::tcp_mux::{TCPMux, TCPMuxDefault};
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCIceCandidateInit,
    RTCIceGatheringState, RTCPeerConnectionState,
};
use webrtc::runtime::{Runtime, Sender, channel};

mod common;
use common::{block_on, runtime, sleep, timeout};

const TEST_MESSAGE: &str = "Hello over the shared TCP port!";
const ECHO_MESSAGE: &str = "Echo over the shared TCP port!";

struct PeerHandler {
    gather_complete_tx: Sender<()>,
    connected_tx: Sender<()>,
    msg_tx: Sender<String>,
    echo: bool,
    runtime: Arc<dyn Runtime>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for PeerHandler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            let _ = self.gather_complete_tx.try_send(());
        }
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        if state == RTCPeerConnectionState::Connected {
            let _ = self.connected_tx.try_send(());
        }
    }

    async fn on_data_channel(&self, dc: Arc<dyn DataChannel>) {
        let msg_tx = self.msg_tx.clone();
        let echo = self.echo;
        self.runtime.spawn(Box::pin(async move {
            while let Some(event) = dc.poll().await {
                match event {
                    DataChannelEvent::OnMessage(msg) => {
                        let data = String::from_utf8(msg.data.to_vec()).unwrap_or_default();
                        msg_tx.try_send(data).ok();
                        if echo && let Err(e) = dc.send_text(ECHO_MESSAGE).await {
                            log::error!("failed to echo: {e}");
                        }
                    }
                    DataChannelEvent::OnClose => break,
                    _ => {}
                }
            }
        }));
    }
}

struct TestPeer {
    pc: Arc<dyn PeerConnection>,
    connected_rx: webrtc::runtime::Receiver<()>,
    gather_rx: webrtc::runtime::Receiver<()>,
    msg_rx: webrtc::runtime::Receiver<String>,
    dc_open_rx: webrtc::runtime::Receiver<()>,
    dc: Option<Arc<dyn DataChannel>>,
}

/// Build a peer restricted to ICE-TCP. `mux == Some` rides the shared listener (the server
/// role, passive candidates only from the mux port); `None` gets a per-connection listener
/// (the client role, which actively dials the server's passive candidate).
async fn build_peer(
    mux: Option<&Arc<TCPMuxDefault>>,
    echo: bool,
    create_dc: bool,
) -> Result<TestPeer> {
    let runtime = runtime();
    let (gather_complete_tx, gather_rx) = channel::<()>(1);
    let (connected_tx, connected_rx) = channel::<()>(1);
    let (msg_tx, msg_rx) = channel::<String>(8);
    let (dc_open_tx, dc_open_rx) = channel::<()>(1);

    let builder = PeerConnectionBuilder::<String>::new()
        .with_handler(Arc::new(PeerHandler {
            gather_complete_tx,
            connected_tx,
            msg_tx: msg_tx.clone(),
            echo,
            runtime: runtime.clone(),
        }))
        .with_runtime(runtime.clone())
        .with_setting_engine(
            SettingEngineBuilder::new()
                .with_network_types(vec![NetworkType::Tcp4])
                .build(),
        );
    let pc = match mux {
        Some(mux) => builder.with_tcp_mux(mux.clone()).build().await?,
        None => {
            builder
                .with_tcp_addrs(vec!["127.0.0.1:0".to_string()])
                .build()
                .await?
        }
    };
    let pc: Arc<dyn PeerConnection> = Arc::new(pc);

    let dc = if create_dc {
        let dc = pc.create_data_channel("test-channel", None).await?;
        {
            let dc = dc.clone();
            runtime.spawn(Box::pin(async move {
                while let Some(event) = dc.poll().await {
                    match event {
                        DataChannelEvent::OnOpen => {
                            dc_open_tx.try_send(()).ok();
                        }
                        DataChannelEvent::OnMessage(msg) => {
                            let data = String::from_utf8(msg.data.to_vec()).unwrap_or_default();
                            msg_tx.try_send(data).ok();
                        }
                        _ => {}
                    }
                }
            }));
        }
        Some(dc)
    } else {
        None
    };

    Ok(TestPeer {
        pc,
        connected_rx,
        gather_rx,
        msg_rx,
        dc_open_rx,
        dc,
    })
}

/// Offer/answer between a muxed server and its client, then a data-channel round-trip.
async fn exercise_pair(server: &mut TestPeer, client: &mut TestPeer, mux_port: u16) -> Result<()> {
    let offer = client.pc.create_offer(None).await?;
    client.pc.set_local_description(offer).await?;
    timeout(Duration::from_secs(5), client.gather_rx.recv()).await?;
    let offer_sdp = client
        .pc
        .local_description()
        .await
        .expect("client local description");

    server.pc.set_remote_description(offer_sdp).await?;
    let answer = server.pc.create_answer(None).await?;
    server.pc.set_local_description(answer).await?;
    timeout(Duration::from_secs(5), server.gather_rx.recv()).await?;
    let answer_sdp = server
        .pc
        .local_description()
        .await
        .expect("server local description");

    // The muxed side advertises the one shared TCP port for its passive host candidate.
    assert!(
        answer_sdp
            .sdp
            .contains(&format!(" 127.0.0.1 {mux_port} typ host tcptype passive")),
        "server SDP should advertise the mux port as a passive TCP candidate:\n{}",
        answer_sdp.sdp
    );

    client.pc.set_remote_description(answer_sdp.clone()).await?;

    // The facade only dials remote passive TCP candidates when they arrive via
    // `add_ice_candidate` (trickle), not from SDP-embedded candidates — trickle the server's
    // passive candidate the way a WHIP client would.
    for line in answer_sdp.sdp.lines() {
        if let Some(candidate) = line.strip_prefix("a=candidate:")
            && candidate.contains(" tcptype passive")
        {
            client
                .pc
                .add_ice_candidate(RTCIceCandidateInit {
                    candidate: candidate.to_owned(),
                    ..Default::default()
                })
                .await?;
        }
    }

    timeout(Duration::from_secs(15), client.connected_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("client did not connect"))?;
    timeout(Duration::from_secs(5), server.connected_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("server did not connect"))?;

    timeout(Duration::from_secs(10), client.dc_open_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("data channel did not open"))?;

    client
        .dc
        .as_ref()
        .expect("client data channel")
        .send_text(TEST_MESSAGE)
        .await?;

    let received = timeout(Duration::from_secs(10), server.msg_rx.recv())
        .await?
        .expect("server message");
    assert_eq!(received, TEST_MESSAGE);

    let echo = timeout(Duration::from_secs(10), client.msg_rx.recv())
        .await?
        .expect("client echo");
    assert_eq!(echo, ECHO_MESSAGE);

    Ok(())
}

/// The headline case: two connections share one fixed-port listener and each serves its own
/// client over ICE-TCP.
#[test]
fn test_tcp_mux_two_connections_share_one_fixed_port() {
    block_on(async {
        env_logger::builder()
            .filter_level(log::LevelFilter::Info)
            .is_test(true)
            .try_init()
            .ok();

        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        let mux = TCPMuxDefault::new(runtime(), listener)?;
        let mux_port = mux.local_addr()?.port();
        log::info!("TCP mux listening on port {mux_port}");

        // Two server-side connections on the one listener; per-connection binds of the same
        // port would fail the second one with EADDRINUSE.
        let mut server_a = build_peer(Some(&mux), true, false).await?;
        let mut server_b = build_peer(Some(&mux), true, false).await?;
        let mut client_a = build_peer(None, false, true).await?;
        let mut client_b = build_peer(None, false, true).await?;

        exercise_pair(&mut server_a, &mut client_a, mux_port).await?;
        log::info!("pair A connected and exchanged messages");
        exercise_pair(&mut server_b, &mut client_b, mux_port).await?;
        log::info!("pair B connected and exchanged messages");

        // Both pairs are live at once: pair A still answers after pair B connected.
        client_a
            .dc
            .as_ref()
            .expect("client a data channel")
            .send_text(TEST_MESSAGE)
            .await?;
        let received = timeout(Duration::from_secs(10), server_a.msg_rx.recv())
            .await?
            .expect("server a still receives");
        assert_eq!(received, TEST_MESSAGE);

        sleep(Duration::from_millis(100)).await;
        server_a.pc.close().await?;
        server_b.pc.close().await?;
        client_a.pc.close().await?;
        client_b.pc.close().await?;
        mux.close();

        Ok::<(), anyhow::Error>(())
    })
    .unwrap();
}

/// `with_tcp_addrs` and `with_tcp_mux` are mutually exclusive: the mux owns the listener, so
/// there is nothing left for a per-connection bind. `build` must say so.
#[test]
fn test_tcp_mux_conflicts_with_tcp_addrs() {
    block_on(async {
        struct NoopHandler;
        #[async_trait::async_trait]
        impl PeerConnectionEventHandler for NoopHandler {}

        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        let mux = TCPMuxDefault::new(runtime(), listener)?;

        let result = PeerConnectionBuilder::new()
            .with_handler(Arc::new(NoopHandler))
            .with_runtime(runtime())
            .with_tcp_addrs(vec!["127.0.0.1:0".to_string()])
            .with_tcp_mux(mux.clone())
            .build()
            .await;

        assert!(result.is_err(), "tcp_addrs + tcp_mux must fail to build");
        mux.close();
        Ok::<(), anyhow::Error>(())
    })
    .unwrap();
}

/// Application-pinned ICE credentials are honoured on a TCP mux, but they must be unique per
/// connection: a second registration under the same ufrag fails `build`, and closing the
/// first connection frees the ufrag for reuse.
#[test]
fn test_tcp_mux_pinned_credentials_are_unique_and_freed_on_close() {
    block_on(async {
        struct NoopHandler;
        #[async_trait::async_trait]
        impl PeerConnectionEventHandler for NoopHandler {}

        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        let mux = TCPMuxDefault::new(runtime(), listener)?;

        let pinned_engine = || {
            SettingEngineBuilder::new()
                .with_ice_credentials(
                    "pinnedufrag".to_owned(),
                    "pinned-password-32bytes-padded".to_owned(),
                )
                .build()
        };

        let build = |mux: Arc<TCPMuxDefault>| {
            let engine = pinned_engine();
            async move {
                PeerConnectionBuilder::<String>::new()
                    .with_handler(Arc::new(NoopHandler))
                    .with_runtime(runtime())
                    .with_setting_engine(engine)
                    .with_tcp_mux(mux)
                    .build()
                    .await
            }
        };

        let first = match build(mux.clone()).await {
            Ok(pc) => pc,
            Err(e) => panic!("first registration should succeed: {e}"),
        };

        let second = build(mux.clone()).await;
        assert!(
            second.is_err(),
            "the same pinned ufrag twice must be refused"
        );

        first.close().await?;

        let third = match build(mux.clone()).await {
            Ok(pc) => pc,
            Err(e) => panic!("closing the first connection frees its ufrag: {e}"),
        };
        third.close().await?;
        mux.close();

        Ok::<(), anyhow::Error>(())
    })
    .unwrap();
}
