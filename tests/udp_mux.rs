/// Integration tests for UDP multiplexing: several peer connections on one shared socket.
///
/// The headline property: two connections bound to the **same fixed port** coexist, which is
/// impossible with per-connection binds (`with_udp_addrs` on one port fails the second bind
/// with `EADDRINUSE`) and is the whole point of [`UDPMux`].
use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;

use rtc::peer_connection::configuration::setting_engine::SettingEngineBuilder;
use webrtc::data_channel::{DataChannel, DataChannelEvent};
use webrtc::peer_connection::transport::udp_mux::{UDPMux, UDPMuxDefault};
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCIceGatheringState,
    RTCPeerConnectionState,
};
use webrtc::runtime::{Runtime, Sender, channel};

mod common;
use common::{block_on, runtime, sleep, timeout};

const TEST_MESSAGE: &str = "Hello over the shared port!";
const ECHO_MESSAGE: &str = "Echo over the shared port!";

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

/// One peer of the test: its connection plus the receivers its handler reports on.
struct TestPeer {
    pc: Arc<dyn PeerConnection>,
    connected_rx: webrtc::runtime::Receiver<()>,
    gather_rx: webrtc::runtime::Receiver<()>,
    msg_rx: webrtc::runtime::Receiver<String>,
    dc_open_rx: webrtc::runtime::Receiver<()>,
    dc: Option<Arc<dyn DataChannel>>,
}

/// Build a peer. `mux == None` builds a plain per-connection-socket peer (the client role);
/// `Some` rides the shared socket (the server role). Muxed peers run ICE Lite, the
/// deployment shape this feature is for (SRS, mediamtx, …): a lite agent answers checks
/// without initiating them and gathers host candidates only — which is also all a muxed
/// connection can gather.
///
/// The two roles must differ: two muxed connections talking *to each other* through one mux
/// share a 5-tuple — both candidates are the mux address itself — which no demultiplexer can
/// tell apart once checks give way to ufrag-less traffic. That is fine in practice: muxing is
/// a server-side deployment shape, and the clients are elsewhere.
async fn build_peer(
    mux: Option<&Arc<UDPMuxDefault>>,
    echo: bool,
    create_dc: bool,
    setting_engine: Option<webrtc::peer_connection::SettingEngine>,
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
        .with_runtime(runtime.clone());
    let pc = match mux {
        Some(mux) => {
            let engine = setting_engine
                .unwrap_or_else(|| SettingEngineBuilder::new().with_lite(true).build());
            builder
                .with_setting_engine(engine)
                .with_udp_mux(mux.clone())
                .build()
                .await?
        }
        None => {
            let builder = match setting_engine {
                Some(engine) => builder.with_setting_engine(engine),
                None => builder,
            };
            builder
                .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
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

/// Offer/answer between a (muxed) answering server and a (plain) offering client, then a
/// message round-trip on the client's data channel.
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

    client.pc.set_remote_description(answer_sdp.clone()).await?;

    // The muxed side advertises the one shared port as its host candidate.
    assert!(
        answer_sdp
            .sdp
            .contains(&format!("127.0.0.1 {mux_port} typ host")),
        "server SDP should advertise the mux port:
{}",
        answer_sdp.sdp
    );

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

/// The headline case: two connections share one fixed-port socket — impossible with
/// per-connection binds — and each serves its own client concurrently.
#[test]
fn test_udp_mux_two_connections_share_one_fixed_port() {
    block_on(async {
        env_logger::builder()
            .filter_level(log::LevelFilter::Info)
            .is_test(true)
            .try_init()
            .ok();

        let socket = std::net::UdpSocket::bind("127.0.0.1:0")?;
        let mux = UDPMuxDefault::new(runtime(), socket)?;
        let mux_port = mux.local_addr()?.port();
        log::info!("UDP mux listening on port {mux_port}");

        // Two server-side connections on the one socket; per-connection binds of the same
        // port would fail the second one with EADDRINUSE.
        let mut server_a = build_peer(Some(&mux), true, false, None).await?;
        let mut server_b = build_peer(Some(&mux), true, false, None).await?;
        let mut client_a = build_peer(None, false, true, None).await?;
        let mut client_b = build_peer(None, false, true, None).await?;

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

/// `with_udp_addrs` and `with_udp_mux` are mutually exclusive: the mux owns the socket, so
/// there is nothing left for a per-connection bind. `build` must say so.
#[test]
fn test_udp_mux_conflicts_with_udp_addrs() {
    block_on(async {
        let socket = std::net::UdpSocket::bind("127.0.0.1:0")?;
        let mux = UDPMuxDefault::new(runtime(), socket)?;

        struct NoopHandler;
        #[async_trait::async_trait]
        impl PeerConnectionEventHandler for NoopHandler {}

        let result = PeerConnectionBuilder::new()
            .with_handler(Arc::new(NoopHandler))
            .with_runtime(runtime())
            .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
            .with_udp_mux(mux.clone())
            .build()
            .await;

        assert!(result.is_err(), "udp_addrs + udp_mux must fail to build");
        mux.close();
        Ok::<(), anyhow::Error>(())
    })
    .unwrap();
}

/// Application-pinned ICE credentials are honoured on a mux, but they must be unique per
/// connection: a second registration under the same ufrag fails `build`, and closing the
/// first connection frees the ufrag for reuse.
#[test]
fn test_udp_mux_pinned_credentials_are_unique_and_freed_on_close() {
    block_on(async {
        struct NoopHandler;
        #[async_trait::async_trait]
        impl PeerConnectionEventHandler for NoopHandler {}

        let socket = std::net::UdpSocket::bind("127.0.0.1:0")?;
        let mux = UDPMuxDefault::new(runtime(), socket)?;

        let pinned_engine = || {
            SettingEngineBuilder::new()
                .with_ice_credentials(
                    "pinnedufrag".to_owned(),
                    "pinned-password-32bytes-padded".to_owned(),
                )
                .build()
        };

        let build = |mux: Arc<UDPMuxDefault>| {
            let engine = pinned_engine();
            async move {
                PeerConnectionBuilder::<String>::new()
                    .with_handler(Arc::new(NoopHandler))
                    .with_runtime(runtime())
                    .with_setting_engine(engine)
                    .with_udp_mux(mux)
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

/// An ICE restart on a muxed connection keeps the shared port but rotates the ufrag: the
/// muxed side re-registers under the freshly rotated credentials (the old ufrag stays routable
/// for a grace period), and the advertised candidates stay on the shared port across the
/// restart.
#[test]
fn test_udp_mux_ice_restart_rotates_ufrag_and_keeps_port() {
    // With the rebind opted into, the restart re-attaches to the existing mux registration
    // instead of rebinding the shared socket.
    run_mux_ice_restart(true);
}

/// Same as above, but with the default setting: no rebind is armed for the restart, so the
/// credential rotation reaches the mux through the credential sync that precedes the restarted
/// gathering — no `bind_transports` run is involved.
#[test]
fn test_udp_mux_ice_restart_rotates_ufrag_without_rebind() {
    run_mux_ice_restart(false);
}

fn run_mux_ice_restart(discard_local_candidates: bool) {
    block_on(async {
        env_logger::builder()
            .filter_level(log::LevelFilter::Info)
            .is_test(true)
            .try_init()
            .ok();

        let socket = std::net::UdpSocket::bind("127.0.0.1:0")?;
        let mux = UDPMuxDefault::new(runtime(), socket)?;
        let mux_port = mux.local_addr()?.port();

        // The muxed peer offers (full ICE — ICE Lite agents cannot initiate a restart). The
        // discard setting decides whether the restart also rebinds transports.
        let (gather_complete_tx, mut gather_rx) = channel::<()>(1);
        let (connected_tx, mut connected_rx) = channel::<()>(2);
        let (msg_tx, _msg_rx) = channel::<String>(8);
        let offerer = PeerConnectionBuilder::<String>::new()
            .with_handler(Arc::new(PeerHandler {
                gather_complete_tx,
                connected_tx,
                msg_tx,
                echo: false,
                runtime: runtime(),
            }))
            .with_runtime(runtime())
            .with_setting_engine(
                SettingEngineBuilder::new()
                    .with_discard_local_candidates_during_ice_restart(discard_local_candidates)
                    .build(),
            )
            .with_udp_mux(mux.clone())
            .build()
            .await?;
        let offerer_dc = offerer.create_data_channel("test-channel", None).await?;
        let (dc_open_tx, mut dc_open_rx) = channel::<()>(1);
        {
            let dc = offerer_dc.clone();
            runtime().spawn(Box::pin(async move {
                while let Some(event) = dc.poll().await {
                    if let DataChannelEvent::OnOpen = event {
                        dc_open_tx.try_send(()).ok();
                    }
                }
            }));
        }

        let mut answerer = build_peer(None, true, false, None).await?;

        let offer = offerer.create_offer(None).await?;
        let ufrag_before = offer
            .sdp
            .lines()
            .find(|line| line.starts_with("a=ice-ufrag:"))
            .map(str::to_owned)
            .expect("offer carries an ice-ufrag");
        offerer.set_local_description(offer).await?;
        timeout(Duration::from_secs(5), gather_rx.recv()).await?;
        let offer_sdp = offerer
            .local_description()
            .await
            .expect("offerer local description");

        answerer.pc.set_remote_description(offer_sdp).await?;
        let answer = answerer.pc.create_answer(None).await?;
        answerer.pc.set_local_description(answer).await?;
        timeout(Duration::from_secs(5), answerer.gather_rx.recv()).await?;
        let answer_sdp = answerer
            .pc
            .local_description()
            .await
            .expect("answerer local description");
        offerer.set_remote_description(answer_sdp).await?;

        timeout(Duration::from_secs(15), connected_rx.recv())
            .await
            .map_err(|_| anyhow::anyhow!("muxed offerer did not connect"))?;
        timeout(Duration::from_secs(5), answerer.connected_rx.recv())
            .await
            .map_err(|_| anyhow::anyhow!("answerer did not connect"))?;

        timeout(Duration::from_secs(10), dc_open_rx.recv())
            .await
            .map_err(|_| anyhow::anyhow!("data channel did not open"))?;
        offerer_dc.send_text(TEST_MESSAGE).await?;
        let received = timeout(Duration::from_secs(10), answerer.msg_rx.recv())
            .await?
            .expect("answerer message");
        assert_eq!(received, TEST_MESSAGE);

        // Restart on the muxed side.
        offerer.restart_ice().await?;
        let restart_offer = offerer.create_offer(None).await?;
        let ufrag_after = restart_offer
            .sdp
            .lines()
            .find(|line| line.starts_with("a=ice-ufrag:"))
            .map(str::to_owned)
            .expect("restart offer carries an ice-ufrag");
        assert_ne!(
            ufrag_before, ufrag_after,
            "an ICE restart rotates the ufrag — the mux re-registers under the new one"
        );
        assert!(
            restart_offer
                .sdp
                .contains(&format!("127.0.0.1 {mux_port} typ host")),
            "restart offer still advertises the shared port:\n{}",
            restart_offer.sdp
        );

        offerer.set_local_description(restart_offer).await?;
        timeout(Duration::from_secs(5), gather_rx.recv()).await?;
        let restart_offer_sdp = offerer
            .local_description()
            .await
            .expect("offerer local description after restart");
        answerer
            .pc
            .set_remote_description(restart_offer_sdp)
            .await?;
        let answer = answerer.pc.create_answer(None).await?;
        answerer.pc.set_local_description(answer).await?;
        timeout(Duration::from_secs(5), answerer.gather_rx.recv()).await?;
        let answer_sdp = answerer
            .pc
            .local_description()
            .await
            .expect("answerer local description after restart");
        offerer.set_remote_description(answer_sdp).await?;

        // Still connected, still chatting, still the one port.
        timeout(Duration::from_secs(15), connected_rx.recv())
            .await
            .map_err(|_| anyhow::anyhow!("muxed offerer did not reconnect"))?;
        offerer_dc.send_text(TEST_MESSAGE).await?;
        let received = timeout(Duration::from_secs(10), answerer.msg_rx.recv())
            .await?
            .expect("answerer message after restart");
        assert_eq!(received, TEST_MESSAGE);

        sleep(Duration::from_millis(100)).await;
        offerer.close().await?;
        answerer.pc.close().await?;
        mux.close();

        Ok::<(), anyhow::Error>(())
    })
    .unwrap();
}

/// The regression from review: a duplicate-ufrag build fails *while the first connection is
/// live*, and its failed driver's Drop must not evict the first connection's registration.
///
/// Server 1 (pinned ufrag, muxed) connects to a client and exchanges a message; a duplicate
/// build fails; server 1 must still pass traffic.
#[test]
fn test_udp_mux_failed_duplicate_build_keeps_the_live_connection() {
    block_on(async {
        env_logger::builder()
            .filter_level(log::LevelFilter::Info)
            .is_test(true)
            .try_init()
            .ok();

        let socket = std::net::UdpSocket::bind("127.0.0.1:0")?;
        let mux = UDPMuxDefault::new(runtime(), socket)?;
        let mux_port = mux.local_addr()?.port();

        let pinned = || {
            SettingEngineBuilder::new()
                .with_lite(true)
                .with_ice_credentials(
                    "pinnedufrag".to_owned(),
                    "pinned-password-32bytes-padded".to_owned(),
                )
                .build()
        };

        let mut server = build_peer(Some(&mux), true, false, Some(pinned())).await?;
        let mut client = build_peer(None, false, true, None).await?;
        exercise_pair(&mut server, &mut client, mux_port).await?;

        // A duplicate build under the live connection's ufrag fails...
        let dup = PeerConnectionBuilder::<String>::new()
            .with_handler(Arc::new(PeerHandler {
                gather_complete_tx: channel::<()>(1).0,
                connected_tx: channel::<()>(1).0,
                msg_tx: channel::<String>(1).0,
                echo: false,
                runtime: runtime(),
            }))
            .with_runtime(runtime())
            .with_setting_engine(pinned())
            .with_udp_mux(mux.clone())
            .build()
            .await;
        assert!(dup.is_err(), "the duplicate ufrag must be refused");
        drop(dup);

        // ...and the first connection is none the worse for it.
        client
            .dc
            .as_ref()
            .expect("client data channel")
            .send_text(TEST_MESSAGE)
            .await?;
        let received = timeout(Duration::from_secs(10), server.msg_rx.recv())
            .await?
            .expect("the live connection still receives after the failed duplicate");
        assert_eq!(received, TEST_MESSAGE);

        server.pc.close().await?;
        client.pc.close().await?;
        mux.close();

        Ok::<(), anyhow::Error>(())
    })
    .unwrap();
}

/// The wildcard path (the most platform-sensitive part): a mux bound to `0.0.0.0` advertises
/// per-interface candidates on its port, and inbound packets are attributed to the right one
/// by `dst_ip` (packet info) — the loopback-only tests never exercise either.
///
/// Skipped when the host has no usable non-loopback IPv4 interface.
#[test]
fn test_udp_mux_wildcard_listen_and_dst_ip_attribution() {
    block_on(async {
        env_logger::builder()
            .filter_level(log::LevelFilter::Info)
            .is_test(true)
            .try_init()
            .ok();

        // A usable interface of the wildcard's family: what expand_wildcard would advertise.
        let nic = rtc::shared::ifaces::ifaces()
            .map_err(anyhow::Error::from)?
            .into_iter()
            .filter_map(|interface| interface.addr)
            .map(|addr| addr.ip())
            .find(|ip| match ip {
                // Usable for a candidate: not loopback, not unspecified, not link-local
                // (169.254/16 — IpAddr::is_link_local is unstable).
                std::net::IpAddr::V4(v4) => {
                    !v4.is_loopback() && !v4.is_unspecified() && !v4.is_link_local()
                }
                std::net::IpAddr::V6(_) => false,
            });
        let Some(nic) = nic else {
            eprintln!("no usable non-loopback IPv4 interface; skipping the wildcard test");
            return Ok(());
        };
        log::info!("wildcard test over interface {nic}");

        let socket = std::net::UdpSocket::bind("0.0.0.0:0")?;
        let mux = UDPMuxDefault::new(runtime(), socket)?;
        let mux_port = mux.local_addr()?.port();

        let mut server = build_peer(Some(&mux), true, false, None).await?;
        // The client binds a wildcard, so it has a candidate on the NIC (a loopback-bound
        // socket could not form a pair with the server's NIC candidate).
        let mut client = build_peer_wildcard().await?;

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

        // Wildcard expansion: the NIC address on the mux port is among the host candidates.
        assert!(
            answer_sdp
                .sdp
                .contains(&format!("{nic} {mux_port} typ host")),
            "server SDP should advertise {nic}:{mux_port}:\n{}",
            answer_sdp.sdp
        );

        client.pc.set_remote_description(answer_sdp).await?;

        timeout(Duration::from_secs(15), client.connected_rx.recv())
            .await
            .map_err(|_| anyhow::anyhow!("client did not connect over {nic}"))?;
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

        sleep(Duration::from_millis(100)).await;
        server.pc.close().await?;
        client.pc.close().await?;
        mux.close();

        Ok::<(), anyhow::Error>(())
    })
    .unwrap();
}

/// A client peer bound to the wildcard, so it offers a candidate on the host's real interface.
async fn build_peer_wildcard() -> Result<TestPeer> {
    let runtime = runtime();
    let (gather_complete_tx, gather_rx) = channel::<()>(1);
    let (connected_tx, connected_rx) = channel::<()>(1);
    let (msg_tx, msg_rx) = channel::<String>(8);
    let (dc_open_tx, dc_open_rx) = channel::<()>(1);

    let pc = PeerConnectionBuilder::<String>::new()
        .with_handler(Arc::new(PeerHandler {
            gather_complete_tx,
            connected_tx,
            msg_tx: msg_tx.clone(),
            echo: false,
            runtime: runtime.clone(),
        }))
        .with_runtime(runtime.clone())
        .with_udp_addrs(vec!["0.0.0.0:0".to_string()])
        .build()
        .await?;
    let pc: Arc<dyn PeerConnection> = Arc::new(pc);

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

    Ok(TestPeer {
        pc,
        connected_rx,
        gather_rx,
        msg_rx,
        dc_open_rx,
        dc: Some(dc),
    })
}
