//! A wildcard bind address means "every local interface", not the wildcard socket (webrtc#874).
//!
//! Host candidates are derived from the socket's local address, so a connection that bound
//! `0.0.0.0:0` verbatim advertised `0.0.0.0` — an address no peer can dial, leaving it dependent on
//! srflx/relay for a path that should have been direct. The addresses are also resolved on every
//! bind rather than once at construction, which is what lets an ICE-restart rebind follow a
//! network handover instead of asking for an interface that has gone away.
//!
//! What the interfaces actually are depends on the machine, so these tests assert the properties
//! that must hold anywhere, and skip on a host that has no usable interface at all (CI containers
//! with only loopback) rather than assert on a number they did not really measure.

use anyhow::Result;
use std::collections::BTreeSet;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;
use webrtc::data_channel::RTCDataChannelInit;
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCIceGatheringState,
};
use webrtc::runtime::{Sender, channel};

use rtc::peer_connection::configuration::setting_engine::SettingEngine;
use rtc::shared::ifaces::ifaces;

mod common;
use common::{block_on, runtime, timeout};

struct Handler {
    gather_tx: Sender<()>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for Handler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            let _ = self.gather_tx.try_send(());
        }
    }
}

/// A [`SettingEngine`] that opts into replacing the transport on an ICE restart.
///
/// 0.20 configures the engine through setters rather than 0.21's `SettingEngineBuilder`.
fn rebinding_setting_engine() -> SettingEngine {
    let mut setting_engine = SettingEngine::default();
    setting_engine.set_discard_local_candidates_during_ice_restart(true);
    setting_engine
}

/// The IPv4 addresses a `0.0.0.0` bind is expected to expand into on this machine.
fn usable_ipv4_interfaces() -> BTreeSet<IpAddr> {
    ifaces()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|interface| interface.addr.map(|addr| addr.ip()))
        .filter(|ip| {
            matches!(ip, IpAddr::V4(v4) if !v4.is_loopback() && !v4.is_unspecified() && !v4.is_link_local())
        })
        .collect()
}

/// The IP addresses of the UDP host candidates in an offer.
fn udp_host_candidate_ips(sdp: &str) -> BTreeSet<IpAddr> {
    udp_host_candidates(sdp)
        .into_iter()
        .map(|(ip, _port)| ip)
        .collect()
}

/// The UDP host candidates in an offer, as address/port pairs.
///
/// Read out of the SDP rather than the transport graph: gathering finishes long before anything
/// is negotiated, and the transports that walk would go through do not exist until then.
///
/// `a=candidate:<foundation> <component> <transport> <priority> <address> <port> typ <type>`
fn udp_host_candidates(sdp: &str) -> Vec<(IpAddr, u16)> {
    sdp.lines()
        .filter_map(|line| line.trim().strip_prefix("a=candidate:"))
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            let [
                _foundation,
                _component,
                transport,
                _priority,
                address,
                port,
                "typ",
                "host",
                ..,
            ] = fields[..]
            else {
                return None;
            };
            if !transport.eq_ignore_ascii_case("udp") {
                return None;
            }
            Some((address.parse::<IpAddr>().ok()?, port.parse::<u16>().ok()?))
        })
        .collect()
}

/// Gather on a wildcard and return the UDP host candidate addresses it produced.
async fn gather_on(udp_addr: &str) -> Result<BTreeSet<IpAddr>> {
    let (gather_tx, mut gather_rx) = channel::<()>(1);

    let pc = PeerConnectionBuilder::new()
        .with_handler(Arc::new(Handler { gather_tx }))
        .with_runtime(runtime())
        .with_setting_engine(SettingEngine::default())
        .with_udp_addrs(vec![udp_addr.to_string()])
        .build()
        .await?;

    let _dc = pc
        .create_data_channel("wildcard", Some(RTCDataChannelInit::default()))
        .await?;
    let offer = pc.create_offer(None).await?;
    pc.set_local_description(offer).await?;
    timeout(Duration::from_secs(10), gather_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("gathering did not complete"))?;

    let offer = pc
        .local_description()
        .await
        .ok_or_else(|| anyhow::anyhow!("no local description"))?;
    let ips = udp_host_candidate_ips(&offer.sdp);
    pc.close().await?;
    Ok(ips)
}

/// `0.0.0.0:0` gathers one host candidate per local IPv4 interface — and never `0.0.0.0`.
async fn wildcard_gathers_interface_host_candidates() -> Result<()> {
    let expected = usable_ipv4_interfaces();
    if expected.is_empty() {
        eprintln!("skipping: no usable IPv4 interface on this host");
        return Ok(());
    }

    let gathered = gather_on("0.0.0.0:0").await?;

    assert!(
        !gathered.iter().any(|ip| ip.is_unspecified()),
        "a wildcard must not be advertised as a host candidate: {gathered:?}"
    );
    assert_eq!(
        gathered, expected,
        "0.0.0.0 gathers exactly the machine's usable IPv4 interfaces"
    );
    Ok(())
}

/// An ICE-restart rebind re-runs the expansion instead of reusing what it resolved at build time.
///
/// This is the shape of the Android failure in webrtc#874: the rebind asked for the address the
/// connection was built with, which after a handover no longer exists on the device, and the
/// driver exited. A test cannot take an interface away, so it asserts the mechanism that makes
/// the recovery possible — the restarted generation's candidates come from a *fresh* enumeration
/// (they follow the interfaces as they are now) on *fresh* sockets (the ports all changed).
async fn ice_restart_re_enumerates_interfaces() -> Result<()> {
    let expected = usable_ipv4_interfaces();
    if expected.is_empty() {
        eprintln!("skipping: no usable IPv4 interface on this host");
        return Ok(());
    }

    let (gather_tx, mut gather_rx) = channel::<()>(1);
    let pc = PeerConnectionBuilder::new()
        .with_handler(Arc::new(Handler { gather_tx }))
        .with_runtime(runtime())
        .with_setting_engine(rebinding_setting_engine())
        .with_udp_addrs(vec!["0.0.0.0:0".to_string()])
        .build()
        .await?;

    let _dc = pc
        .create_data_channel("wildcard", Some(RTCDataChannelInit::default()))
        .await?;
    let offer = pc.create_offer(None).await?;
    pc.set_local_description(offer).await?;
    timeout(Duration::from_secs(10), gather_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("gathering did not complete"))?;
    let first = pc.local_description().await.expect("offer").sdp;

    pc.restart_ice().await?;
    let restart_offer = pc.create_offer(None).await?;
    // A rebind that failed takes the driver with it, and this is where that shows up.
    pc.set_local_description(restart_offer)
        .await
        .map_err(|err| anyhow::anyhow!("driver died rebinding a wildcard: {err}"))?;
    timeout(Duration::from_secs(10), gather_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("no re-gather after the restart: the rebind failed"))?;
    let second = pc.local_description().await.expect("restart offer").sdp;

    assert_eq!(
        udp_host_candidate_ips(&first),
        expected,
        "the first generation gathered the machine's interfaces"
    );

    // The restarted generation's candidates are the ones on ports that did not exist before —
    // `:0` gives every rebound socket a fresh port. Identifying them this way rather than by
    // diffing the two descriptions is what keeps the assertion about the *new* sockets: nothing
    // requires the peer to drop the previous generation's candidates from its description, and
    // this connection is deliberately never negotiated, so they are still listed.
    let ports_before: BTreeSet<u16> = udp_host_candidates(&first)
        .into_iter()
        .map(|(_ip, port)| port)
        .collect();
    let regathered: BTreeSet<IpAddr> = udp_host_candidates(&second)
        .into_iter()
        .filter(|(_ip, port)| !ports_before.contains(port))
        .map(|(ip, _port)| ip)
        .collect();
    assert_eq!(
        regathered, expected,
        "the restart bound fresh sockets and enumerated the interfaces again"
    );

    pc.close().await?;
    Ok(())
}

/// A concrete address still binds exactly that interface — the expansion must not leak into it.
async fn concrete_address_binds_only_itself() -> Result<()> {
    let gathered = gather_on("127.0.0.1:0").await?;
    assert_eq!(
        gathered,
        BTreeSet::from(["127.0.0.1".parse::<IpAddr>().unwrap()]),
        "a configured loopback address gathers loopback and nothing else"
    );
    Ok(())
}

#[test]
fn test_wildcard_gathers_interface_host_candidates() {
    block_on(wildcard_gathers_interface_host_candidates()).unwrap();
}

#[test]
fn test_ice_restart_re_enumerates_interfaces() {
    block_on(ice_restart_re_enumerates_interfaces()).unwrap();
}

#[test]
fn test_concrete_address_binds_only_itself() {
    block_on(concrete_address_binds_only_itself()).unwrap();
}
