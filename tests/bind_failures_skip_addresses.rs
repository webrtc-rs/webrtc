//! An address that cannot be bound is skipped, not fatal — unless nothing binds at all
//! (webrtc#874).
//!
//! A device that changes network leaves a configured address pointing at an interface that no
//! longer exists, and binding it fails with `EADDRNOTAVAIL`. Failing the whole bind for that one
//! address throws away every interface that *is* there, which on an ICE-restart rebind is the
//! transport the restart exists to move onto: the driver exits, and every later restart the
//! application attempts fails with `SendError(IceGathering)`.
//!
//! So the rule these tests pin down is the pair — any address may fail, but the empty result is
//! still an error, because a connection with no transport at all cannot go on.
//!
//! `192.0.2.0/24` (TEST-NET-1, RFC 5737) is the unbindable address: it is documentation space, so
//! it is not assigned to a local interface anywhere and binding it fails on every platform.

use anyhow::Result;
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;
use webrtc::data_channel::RTCDataChannelInit;
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCIceGatheringState,
};
use webrtc::runtime::{Sender, channel};

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

/// The distinct ports of the UDP host candidates in an offer — one per socket that actually
/// bound. A set, because a description may list the same candidate more than once.
fn udp_host_candidate_ports(sdp: &str) -> BTreeSet<u16> {
    sdp.lines()
        .filter_map(|line| line.trim().strip_prefix("a=candidate:"))
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            let [
                _foundation,
                _component,
                transport,
                _priority,
                _address,
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
            port.parse::<u16>().ok()
        })
        .collect()
}

/// One unbindable address must not cost the connection the addresses that do bind.
async fn unbindable_address_is_skipped() -> Result<()> {
    let (gather_tx, mut gather_rx) = channel::<()>(1);

    let pc = PeerConnectionBuilder::new()
        .with_handler(Arc::new(Handler { gather_tx }))
        .with_runtime(runtime())
        // The dead address is listed *first*: a loop that gave up on the first failure would
        // never reach the good one, and ordering it this way is what makes that visible.
        .with_udp_addrs(vec!["192.0.2.1:0".to_string(), "127.0.0.1:0".to_string()])
        .build()
        .await
        .map_err(|err| anyhow::anyhow!("one dead address took the whole connection down: {err}"))?;

    let _dc = pc
        .create_data_channel("skip", Some(RTCDataChannelInit::default()))
        .await?;
    let offer = pc.create_offer(None).await?;
    pc.set_local_description(offer).await?;
    timeout(Duration::from_secs(10), gather_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("gathering did not complete"))?;

    let sdp = pc.local_description().await.expect("offer").sdp;
    let ports = udp_host_candidate_ports(&sdp);
    assert_eq!(
        ports.len(),
        1,
        "exactly the one address that could bind is gathered: {ports:?}"
    );

    pc.close().await?;
    Ok(())
}

/// Nothing bound is still an error — and it surfaces out of `build()`, not as a healthy-looking
/// connection in front of a driver that already gave up.
async fn no_bindable_address_fails_the_build() -> Result<()> {
    let (gather_tx, _gather_rx) = channel::<()>(1);

    let result = PeerConnectionBuilder::new()
        .with_handler(Arc::new(Handler { gather_tx }))
        .with_runtime(runtime())
        .with_udp_addrs(vec!["192.0.2.1:0".to_string(), "192.0.2.2:0".to_string()])
        .build()
        .await;

    let Err(err) = result else {
        anyhow::bail!("a connection with no transport at all was reported as built");
    };
    let message = err.to_string();
    assert!(
        message.contains("no udp_sockets or tcp_listeners available"),
        "the error names what went wrong, got: {message}"
    );
    Ok(())
}

#[test]
fn test_unbindable_address_is_skipped() {
    block_on(unbindable_address_is_skipped()).unwrap();
}

#[test]
fn test_no_bindable_address_fails_the_build() {
    block_on(no_bindable_address_fails_the_build()).unwrap();
}
