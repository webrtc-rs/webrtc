//! An ICE restart replaces the UDP transport when the application asks for it (webrtc#868).
//!
//! The failure this guards against: a platform invalidates the socket or its path while the
//! connection is idle, ICE restart replaces credentials but keeps the dead transport, and the
//! restarted generation checks over it forever — signalling succeeds, ICE never leaves `Checking`,
//! and the application's only recovery is to rebuild the whole `PeerConnection`.
//!
//! Sockets bound to `:0` make the rebind observable: fresh sockets mean fresh ports, so the local
//! candidates of the restarted generation cannot be the ones from before. The test runs both ways
//! — with the setting and without — because "the ports changed" only means something if they stay
//! the same when nobody asked for a rebind.
//!
//! Ports cover what the rebind *created*; they say nothing about what it left behind. A socket the
//! driver forgot is no longer anybody's candidate but is still an open descriptor, so the last
//! test counts descriptors across repeated restarts instead — that is where a leak of either a UDP
//! socket or a TCP listener becomes visible.

use anyhow::Result;
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;
use webrtc::data_channel::RTCDataChannelInit;
use webrtc::peer_connection::{PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler};
use webrtc::peer_connection::{RTCIceGatheringState, RTCPeerConnectionState};
use webrtc::runtime::{Sender, channel};

use rtc::peer_connection::configuration::setting_engine::SettingEngineBuilder;
use rtc::peer_connection::transport::{
    RTCIceProtocol, RTCIceTcpCandidateType, RTCIceTransportState,
};

mod common;
use common::{block_on, runtime, sleep, timeout};

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

/// The local candidate ports the restarting peer is currently advertising.
///
/// Read through the transport graph rather than by parsing SDP, so this asserts what ICE will
/// actually check over.
async fn local_ports(pc: &impl PeerConnection) -> Result<BTreeSet<u16>> {
    let ice = pc
        .sctp()
        .await
        .ok_or_else(|| anyhow::anyhow!("SCTP not negotiated"))?
        .transport()
        .ice_transport();
    Ok(ice
        .get_local_candidates()
        .await?
        .iter()
        .map(|candidate| candidate.port)
        .collect())
}

/// The ports of the peer's passive TCP candidates — i.e. the ports its ICE-TCP listeners are
/// bound to. Active TCP candidates are excluded: they carry the placeholder discard port 9, not a
/// listener port, so they would never change and would mask a listener that was not rebound.
async fn tcp_listener_ports(pc: &impl PeerConnection) -> Result<BTreeSet<u16>> {
    let ice = pc
        .sctp()
        .await
        .ok_or_else(|| anyhow::anyhow!("SCTP not negotiated"))?
        .transport()
        .ice_transport();
    Ok(ice
        .get_local_candidates()
        .await?
        .iter()
        .filter(|candidate| {
            candidate.protocol == RTCIceProtocol::Tcp
                && candidate.tcp_type == RTCIceTcpCandidateType::Passive
        })
        .map(|candidate| candidate.port)
        .collect())
}

/// The ports of the peer's UDP candidates — i.e. the ports its UDP sockets are bound to. Kept
/// separate from [`local_ports`] because that set also contains the discard port 9 of the active
/// TCP candidate, which never changes and would hide a socket that was not rebound.
async fn udp_socket_ports(pc: &impl PeerConnection) -> Result<BTreeSet<u16>> {
    let ice = pc
        .sctp()
        .await
        .ok_or_else(|| anyhow::anyhow!("SCTP not negotiated"))?
        .transport()
        .ice_transport();
    Ok(ice
        .get_local_candidates()
        .await?
        .iter()
        .filter(|candidate| candidate.protocol == RTCIceProtocol::Udp)
        .map(|candidate| candidate.port)
        .collect())
}

/// The number of file descriptors this process holds open.
///
/// Sockets and listeners are descriptors, so this counts them without reaching into the driver:
/// what it measures is whether the old ones were *closed*, which dropping them from the driver's
/// map does not by itself guarantee — an `Arc` clone held anywhere else keeps the descriptor alive.
/// Returns `None` where the fd directory is not readable, which is what makes the caller skip
/// rather than assert on a number it did not really measure.
fn open_fd_count() -> Option<usize> {
    ["/proc/self/fd", "/dev/fd"]
        .iter()
        .find_map(|dir| std::fs::read_dir(dir).ok().map(|entries| entries.count()))
}

async fn wait_ice_connected(pc: &impl PeerConnection) -> Result<()> {
    for _ in 0..200 {
        if let Some(sctp) = pc.sctp().await {
            let state = sctp.transport().ice_transport().state().await?;
            if matches!(
                state,
                RTCIceTransportState::Connected | RTCIceTransportState::Completed
            ) {
                return Ok(());
            }
        }
        sleep(Duration::from_millis(50)).await;
    }
    anyhow::bail!("ICE did not reach Connected")
}

async fn ice_restart_rebinds(discard_local_candidates: bool) -> Result<()> {
    let runtime = runtime();

    let (a_gather_tx, mut a_gather_rx) = channel::<()>(1);
    let (a_conn_tx, mut a_conn_rx) = channel::<()>(1);
    let (b_gather_tx, mut b_gather_rx) = channel::<()>(1);
    let (b_conn_tx, mut _b_conn_rx) = channel::<()>(1);

    // Both peers opt in. The offerer arms explicitly at `restart_ice()`; the answerer is never
    // told a restart is coming and has to notice the core restarting while it applies the offer.
    // `:0` is what makes either rebind observable.
    let setting_engine = || {
        SettingEngineBuilder::new()
            .with_discard_local_candidates_during_ice_restart(discard_local_candidates)
            .build()
    };

    let pc_a = PeerConnectionBuilder::new()
        .with_handler(Arc::new(Handler {
            gather_tx: a_gather_tx,
            connected_tx: a_conn_tx,
        }))
        .with_runtime(runtime.clone())
        .with_setting_engine(setting_engine())
        .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
        .build()
        .await?;

    let _dc = pc_a
        .create_data_channel("rebind", Some(RTCDataChannelInit::default()))
        .await?;

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
        .with_setting_engine(setting_engine())
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
        .map_err(|_| anyhow::anyhow!("timeout: initial connect"))?;

    let a_ports_before = local_ports(&pc_a).await?;
    let b_ports_before = local_ports(&pc_b).await?;
    assert!(
        !a_ports_before.is_empty() && !b_ports_before.is_empty(),
        "precondition: the first generation gathered host candidates on both peers"
    );

    // ---- ICE restart ----
    pc_a.restart_ice().await?;
    let restart_offer = pc_a.create_offer(None).await?;
    pc_a.set_local_description(restart_offer).await?;
    let _ = timeout(Duration::from_secs(5), a_gather_rx.recv()).await;
    let restart_offer_sdp = pc_a.local_description().await.expect("restart offer");

    pc_b.set_remote_description(restart_offer_sdp).await?;
    let restart_answer = pc_b.create_answer(None).await?;
    pc_b.set_local_description(restart_answer).await?;
    let _ = timeout(Duration::from_secs(5), b_gather_rx.recv()).await;
    let restart_answer_sdp = pc_b.local_description().await.expect("restart answer");
    pc_a.set_remote_description(restart_answer_sdp).await?;

    let a_ports_after = local_ports(&pc_a).await?;
    let b_ports_after = local_ports(&pc_b).await?;
    assert!(
        !a_ports_after.is_empty() && !b_ports_after.is_empty(),
        "the restarted generation gathered host candidates on both peers"
    );

    if discard_local_candidates {
        assert!(
            a_ports_before.is_disjoint(&a_ports_after),
            "offerer: every socket replaced, before={a_ports_before:?} after={a_ports_after:?}"
        );
        assert!(
            b_ports_before.is_disjoint(&b_ports_after),
            "answerer: every socket replaced, before={b_ports_before:?} after={b_ports_after:?}"
        );
        // Rebinding is for recovery, not novelty: the restarted generation has to connect over
        // the new sockets, on both sides.
        wait_ice_connected(&pc_a).await?;
        wait_ice_connected(&pc_b).await?;

        // A plain renegotiation afterwards must NOT rebind. This is the case that catches
        // over-arming: applying the *answer* to a restart carries the peer's new credentials, so
        // a detector that fires on "the description changed the credentials" rather than on "the
        // core restarted" arms again here and replaces a working transport mid-session — one
        // negotiation later, where it is hard to attribute.
        let renegotiate_offer = pc_a.create_offer(None).await?;
        pc_a.set_local_description(renegotiate_offer).await?;
        let _ = timeout(Duration::from_secs(5), a_gather_rx.recv()).await;
        let renegotiate_sdp = pc_a.local_description().await.expect("renegotiation offer");

        pc_b.set_remote_description(renegotiate_sdp).await?;
        let renegotiate_answer = pc_b.create_answer(None).await?;
        pc_b.set_local_description(renegotiate_answer).await?;
        let _ = timeout(Duration::from_secs(5), b_gather_rx.recv()).await;
        let renegotiate_answer_sdp = pc_b
            .local_description()
            .await
            .expect("renegotiation answer");
        pc_a.set_remote_description(renegotiate_answer_sdp).await?;

        assert_eq!(
            a_ports_after,
            local_ports(&pc_a).await?,
            "offerer: a renegotiation that is not a restart must reuse the transport"
        );
        assert_eq!(
            b_ports_after,
            local_ports(&pc_b).await?,
            "answerer: a renegotiation that is not a restart must reuse the transport"
        );
    } else {
        assert_eq!(
            a_ports_before, a_ports_after,
            "offerer: without the setting the transport is reused"
        );
        assert_eq!(
            b_ports_before, b_ports_after,
            "answerer: without the setting the transport is reused"
        );
    }

    pc_a.close().await?;
    pc_b.close().await?;
    Ok(())
}

/// Static ICE credentials must not blind the detector.
///
/// `with_ice_credentials` pins the local ufrag/pwd, and `stage_ice_restart` seeds a restart from
/// exactly those — so on a connection configured this way the *local* credentials never change
/// across a restart. Detecting restarts by watching them would silently do nothing here, which is
/// the worst shape of failure for this feature: the setting is on, and nothing rebinds.
///
/// The *answerer* is the peer that pins its credentials, because the answerer is the side whose
/// restart is detected rather than requested: the offerer arms explicitly in `restart_ice()` and
/// would pass this test even with a broken detector.
async fn ice_restart_rebinds_with_static_local_credentials() -> Result<()> {
    let runtime = runtime();

    let (a_gather_tx, mut a_gather_rx) = channel::<()>(1);
    let (a_conn_tx, mut a_conn_rx) = channel::<()>(1);
    let (b_gather_tx, mut b_gather_rx) = channel::<()>(1);
    let (b_conn_tx, mut _b_conn_rx) = channel::<()>(1);

    // pc_a pins its credentials and answers; pc_b keeps random ones and initiates the restart.
    let pinned = SettingEngineBuilder::new()
        .with_discard_local_candidates_during_ice_restart(true)
        .with_ice_credentials(
            "fixedufrag".to_owned(),
            "fixedpasswordlongenough".to_owned(),
        )
        .build();
    let random = SettingEngineBuilder::new()
        .with_discard_local_candidates_during_ice_restart(true)
        .build();

    let pc_a = PeerConnectionBuilder::new()
        .with_handler(Arc::new(Handler {
            gather_tx: a_gather_tx,
            connected_tx: a_conn_tx,
        }))
        .with_runtime(runtime.clone())
        .with_setting_engine(pinned)
        .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
        .build()
        .await?;

    let _dc = pc_a
        .create_data_channel("rebind", Some(RTCDataChannelInit::default()))
        .await?;

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
        .with_setting_engine(random)
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
        .map_err(|_| anyhow::anyhow!("timeout: initial connect"))?;

    let a_ports_before = local_ports(&pc_a).await?;

    // pc_b initiates. pc_a has to *notice* the restart while applying the offer — and its own
    // local credentials are pinned, so they do not move when it restarts.
    pc_b.restart_ice().await?;
    let restart_offer = pc_b.create_offer(None).await?;
    pc_b.set_local_description(restart_offer).await?;
    let _ = timeout(Duration::from_secs(5), b_gather_rx.recv()).await;
    let restart_offer_sdp = pc_b.local_description().await.expect("restart offer");

    pc_a.set_remote_description(restart_offer_sdp).await?;
    let restart_answer = pc_a.create_answer(None).await?;
    pc_a.set_local_description(restart_answer).await?;
    let _ = timeout(Duration::from_secs(5), a_gather_rx.recv()).await;
    let restart_answer_sdp = pc_a.local_description().await.expect("restart answer");
    pc_b.set_remote_description(restart_answer_sdp).await?;

    let a_ports_after = local_ports(&pc_a).await?;
    assert!(
        a_ports_before.is_disjoint(&a_ports_after),
        "an answerer with pinned ICE credentials still rebinds: \
         before={a_ports_before:?} after={a_ports_after:?}"
    );

    pc_a.close().await?;
    pc_b.close().await?;
    Ok(())
}

#[test]
fn test_ice_restart_rebinds_with_static_local_credentials() {
    block_on(ice_restart_rebinds_with_static_local_credentials()).unwrap();
}

/// A fixed configured port must survive a rebind.
///
/// The other tests use `:0`, which hides an ownership bug: the driver's socket map is not the
/// only owner — the event loop keeps a clone per socket and every in-flight receive future owns
/// another. If those are still alive when the new sockets are bound, the old ones are still open;
/// with an ephemeral port that is invisible, and with a fixed port the bind fails with
/// `EADDRINUSE` and takes the whole connection down with it.
///
/// The assertion is deliberately "the connection still works", not "the ports changed" — a fixed
/// port is expected to come back identical.
async fn ice_restart_rebinds_a_fixed_port() -> Result<()> {
    // A port that is free right now. Racy in principle; the window is one bind.
    let probe = std::net::UdpSocket::bind("127.0.0.1:0")?;
    let fixed = probe.local_addr()?;
    drop(probe);

    let runtime = runtime();
    let (a_gather_tx, mut a_gather_rx) = channel::<()>(1);
    let (a_conn_tx, mut a_conn_rx) = channel::<()>(1);
    let (b_gather_tx, mut b_gather_rx) = channel::<()>(1);
    let (b_conn_tx, mut _b_conn_rx) = channel::<()>(1);

    let setting_engine = SettingEngineBuilder::new()
        .with_discard_local_candidates_during_ice_restart(true)
        .build();

    let pc_a = PeerConnectionBuilder::new()
        .with_handler(Arc::new(Handler {
            gather_tx: a_gather_tx,
            connected_tx: a_conn_tx,
        }))
        .with_runtime(runtime.clone())
        .with_setting_engine(setting_engine)
        .with_udp_addrs(vec![fixed.to_string()])
        .build()
        .await?;

    let _dc = pc_a
        .create_data_channel("rebind", Some(RTCDataChannelInit::default()))
        .await?;

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
        .map_err(|_| anyhow::anyhow!("timeout: initial connect"))?;

    pc_a.restart_ice().await?;
    let restart_offer = pc_a.create_offer(None).await?;
    // If the rebind failed, the driver has exited and this send fails — which is how the bug
    // originally showed up.
    pc_a.set_local_description(restart_offer)
        .await
        .map_err(|err| anyhow::anyhow!("driver died rebinding a fixed port: {err}"))?;

    timeout(Duration::from_secs(5), a_gather_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("no re-gather after restart: the rebind failed"))?;

    let ports = local_ports(&pc_a).await?;
    assert!(
        ports.contains(&fixed.port()),
        "the configured fixed port is bound again after the rebind: {ports:?}"
    );

    pc_a.close().await?;
    pc_b.close().await?;
    Ok(())
}

/// One restart replaces *both* transports: the UDP sockets and the ICE-TCP listeners.
///
/// Binding moved into the driver so that startup and rebind share one path; TCP came along for
/// free as a result. "For free" is worth checking rather than asserting — a listener that
/// survived a rebind would keep advertising a port whose accept loop belongs to the previous
/// generation. Asserting both in one restart is the point: a rebind that replaced only the
/// sockets it was written for would still pass a test that looked at one protocol.
///
/// The connection itself runs over UDP; the TCP listener only has to be replaced, which its
/// passive candidate's port makes visible.
async fn ice_restart_rebinds_udp_and_tcp() -> Result<()> {
    let runtime = runtime();

    let (a_gather_tx, mut a_gather_rx) = channel::<()>(1);
    let (a_conn_tx, mut a_conn_rx) = channel::<()>(1);
    let (b_gather_tx, mut b_gather_rx) = channel::<()>(1);
    let (b_conn_tx, mut _b_conn_rx) = channel::<()>(1);

    let setting_engine = || {
        SettingEngineBuilder::new()
            .with_discard_local_candidates_during_ice_restart(true)
            .build()
    };

    let pc_a = PeerConnectionBuilder::new()
        .with_handler(Arc::new(Handler {
            gather_tx: a_gather_tx,
            connected_tx: a_conn_tx,
        }))
        .with_runtime(runtime.clone())
        .with_setting_engine(setting_engine())
        .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
        .with_tcp_addrs(vec!["127.0.0.1:0".to_string()])
        .build()
        .await?;

    let _dc = pc_a
        .create_data_channel("rebind", Some(RTCDataChannelInit::default()))
        .await?;

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
        .with_setting_engine(setting_engine())
        .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
        .with_tcp_addrs(vec!["127.0.0.1:0".to_string()])
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
        .map_err(|_| anyhow::anyhow!("timeout: initial connect"))?;

    let a_udp_before = udp_socket_ports(&pc_a).await?;
    let b_udp_before = udp_socket_ports(&pc_b).await?;
    let a_tcp_before = tcp_listener_ports(&pc_a).await?;
    let b_tcp_before = tcp_listener_ports(&pc_b).await?;
    assert!(
        !a_tcp_before.is_empty() && !b_tcp_before.is_empty(),
        "precondition: both peers gathered a passive TCP candidate"
    );
    assert!(
        !a_udp_before.is_empty() && !b_udp_before.is_empty(),
        "precondition: both peers gathered a UDP candidate"
    );

    pc_a.restart_ice().await?;
    let restart_offer = pc_a.create_offer(None).await?;
    pc_a.set_local_description(restart_offer).await?;
    let _ = timeout(Duration::from_secs(5), a_gather_rx.recv()).await;
    let restart_offer_sdp = pc_a.local_description().await.expect("restart offer");

    pc_b.set_remote_description(restart_offer_sdp).await?;
    let restart_answer = pc_b.create_answer(None).await?;
    pc_b.set_local_description(restart_answer).await?;
    let _ = timeout(Duration::from_secs(5), b_gather_rx.recv()).await;
    let restart_answer_sdp = pc_b.local_description().await.expect("restart answer");
    pc_a.set_remote_description(restart_answer_sdp).await?;

    let a_udp_after = udp_socket_ports(&pc_a).await?;
    let b_udp_after = udp_socket_ports(&pc_b).await?;
    let a_tcp_after = tcp_listener_ports(&pc_a).await?;
    let b_tcp_after = tcp_listener_ports(&pc_b).await?;
    assert!(
        a_udp_before.is_disjoint(&a_udp_after),
        "offerer: UDP sockets replaced, before={a_udp_before:?} after={a_udp_after:?}"
    );
    assert!(
        b_udp_before.is_disjoint(&b_udp_after),
        "answerer: UDP sockets replaced, before={b_udp_before:?} after={b_udp_after:?}"
    );
    assert!(
        a_tcp_before.is_disjoint(&a_tcp_after),
        "offerer: TCP listeners replaced, before={a_tcp_before:?} after={a_tcp_after:?}"
    );
    assert!(
        b_tcp_before.is_disjoint(&b_tcp_after),
        "answerer: TCP listeners replaced, before={b_tcp_before:?} after={b_tcp_after:?}"
    );

    // The UDP path still has to work afterwards — rebinding TCP must not cost the connection.
    wait_ice_connected(&pc_a).await?;

    pc_a.close().await?;
    pc_b.close().await?;
    Ok(())
}

/// Rebinding must *close* what it replaces: repeated ICE restarts must not accumulate sockets or
/// listeners.
///
/// The driver's map is not the only owner of a UDP socket — the event loop keeps a clone per
/// socket, and every in-flight receive future owns another — so dropping the map alone would leave
/// the descriptors open. Nothing in the transport graph reports that, because the leaked sockets
/// are no longer anybody's candidates; they are only visible as file descriptors. Counting them
/// across many restarts is what separates "the driver forgot the old socket" from "the old socket
/// is gone".
///
/// Both peers rebind on every round and both are configured with UDP *and* TCP, so a leak of
/// either kind shows up as growth.
async fn ice_restart_does_not_leak_sockets() -> Result<()> {
    const ROUNDS: usize = 8;

    let Some(_) = open_fd_count() else {
        eprintln!("skipping: no readable fd directory on this platform");
        return Ok(());
    };

    let runtime = runtime();

    let (a_gather_tx, mut a_gather_rx) = channel::<()>(1);
    let (a_conn_tx, mut a_conn_rx) = channel::<()>(1);
    let (b_gather_tx, mut b_gather_rx) = channel::<()>(1);
    let (b_conn_tx, mut _b_conn_rx) = channel::<()>(1);

    let setting_engine = || {
        SettingEngineBuilder::new()
            .with_discard_local_candidates_during_ice_restart(true)
            .build()
    };

    let pc_a = PeerConnectionBuilder::new()
        .with_handler(Arc::new(Handler {
            gather_tx: a_gather_tx,
            connected_tx: a_conn_tx,
        }))
        .with_runtime(runtime.clone())
        .with_setting_engine(setting_engine())
        .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
        .with_tcp_addrs(vec!["127.0.0.1:0".to_string()])
        .build()
        .await?;

    let _dc = pc_a
        .create_data_channel("rebind", Some(RTCDataChannelInit::default()))
        .await?;

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
        .with_setting_engine(setting_engine())
        .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
        .with_tcp_addrs(vec!["127.0.0.1:0".to_string()])
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
        .map_err(|_| anyhow::anyhow!("timeout: initial connect"))?;

    // Measured after the first restart, not before it: the baseline has to be a state that already
    // went through a rebind, so that one-time allocations are not mistaken for per-restart growth.
    let mut baseline = None;

    for round in 0..=ROUNDS {
        pc_a.restart_ice().await?;
        let restart_offer = pc_a.create_offer(None).await?;
        pc_a.set_local_description(restart_offer).await?;
        let _ = timeout(Duration::from_secs(5), a_gather_rx.recv()).await;
        let restart_offer_sdp = pc_a.local_description().await.expect("restart offer");

        pc_b.set_remote_description(restart_offer_sdp).await?;
        let restart_answer = pc_b.create_answer(None).await?;
        pc_b.set_local_description(restart_answer).await?;
        let _ = timeout(Duration::from_secs(5), b_gather_rx.recv()).await;
        let restart_answer_sdp = pc_b.local_description().await.expect("restart answer");
        pc_a.set_remote_description(restart_answer_sdp).await?;

        // Both drivers have to have finished rebinding before the descriptors are counted,
        // otherwise the count catches a moment where old and new sockets both exist.
        sleep(Duration::from_millis(150)).await;

        let open = open_fd_count().expect("fd directory was readable a moment ago");
        match baseline {
            None => baseline = Some(open),
            Some(baseline) => assert!(
                open <= baseline,
                "restart {round} leaked descriptors: {baseline} open after the first rebind, \
                 {open} now — sockets or listeners from a previous generation are still open"
            ),
        }
    }

    // The connection has to survive all of it; a "leak-free" driver that dropped the transport
    // would pass the count and fail here.
    wait_ice_connected(&pc_a).await?;

    pc_a.close().await?;
    pc_b.close().await?;
    Ok(())
}

#[test]
fn test_ice_restart_rebinds_udp_and_tcp() {
    block_on(ice_restart_rebinds_udp_and_tcp()).unwrap();
}

#[test]
fn test_ice_restart_does_not_leak_sockets() {
    block_on(ice_restart_does_not_leak_sockets()).unwrap();
}

#[test]
fn test_ice_restart_rebinds_a_fixed_port() {
    block_on(ice_restart_rebinds_a_fixed_port()).unwrap();
}

#[test]
fn test_ice_restart_rebinds_udp_sockets_when_enabled() {
    block_on(ice_restart_rebinds(true)).unwrap();
}

#[test]
fn test_ice_restart_reuses_sockets_by_default() {
    block_on(ice_restart_rebinds(false)).unwrap();
}
