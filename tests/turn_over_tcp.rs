//! TURN over TCP against a real TURN server (webrtc-rs/webrtc#848).
//!
//! The mock in `ice_test.rs` proves gathering; this proves the data path. Two relay-only peers
//! connect through one TURN server — the offerer reaching it over TCP, the answerer over UDP —
//! and a data-channel message crosses. With both peers limited to relay candidates and the
//! offerer given only a `transport=tcp` URL, the message has no way across but a relay whose
//! allocation was made, refreshed and permitted over a TCP connection.
//!
//! It needs a TURN server that listens on TCP and lets two loopback relays reach each other:
//!
//! ```shell
//! docker run --rm -d --name turn-tcp --network host coturn/coturn:4.6 \
//!     -n --listening-port=3478 --listening-ip=127.0.0.1 --relay-ip=127.0.0.1 \
//!     --lt-cred-mech --user=user:pass --realm=webrtc.rs --allow-loopback-peers \
//!     --min-port=49160 --max-port=49200 --no-cli
//! WEBRTC_TURN_TCP_SERVER=127.0.0.1:3478 cargo test --test turn_over_tcp -- --nocapture
//! ```
//!
//! `WEBRTC_TURN_CREDENTIALS=user:pass` overrides the credentials. Without
//! `WEBRTC_TURN_TCP_SERVER` the test skips, so it never breaks a CI run that has no TURN server.
use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;

use webrtc::data_channel::{DataChannel, DataChannelEvent};
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCConfigurationBuilder,
    RTCIceGatheringState, RTCIceServer, RTCIceTransportPolicy, RTCPeerConnectionState,
};
use webrtc::runtime::{Runtime, Sender, channel};

mod common;
use common::{block_on, runtime, timeout};

const MESSAGE: &str = "over a relay reached by TCP";

struct Handler {
    gather_tx: Sender<()>,
    connected_tx: Sender<()>,
    /// Set on the answerer: where a received message is reported.
    message_tx: Option<Sender<String>>,
    runtime: Arc<dyn Runtime>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for Handler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            let _ = self.gather_tx.try_send(());
        }
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        eprintln!("connection state: {state:?}");
        if state == RTCPeerConnectionState::Connected {
            let _ = self.connected_tx.try_send(());
        }
    }

    async fn on_data_channel(&self, dc: Arc<dyn DataChannel>) {
        let Some(message_tx) = self.message_tx.clone() else {
            return;
        };
        // Must spawn: returning from on_data_channel unblocks the driver.
        self.runtime.spawn(Box::pin(async move {
            while let Some(event) = dc.poll().await {
                match event {
                    DataChannelEvent::OnMessage(msg) => {
                        let text = String::from_utf8_lossy(&msg.data).into_owned();
                        let _ = message_tx.try_send(text);
                    }
                    DataChannelEvent::OnClose | DataChannelEvent::OnError => break,
                    _ => {}
                }
            }
        }));
    }
}

fn relay_only(url: String, username: &str, credential: &str) -> RTCConfigurationBuilder {
    RTCConfigurationBuilder::new()
        .with_ice_servers(vec![RTCIceServer {
            urls: vec![url],
            username: username.to_owned(),
            credential: credential.to_owned(),
        }])
        .with_ice_transport_policy(RTCIceTransportPolicy::Relay)
}

#[test]
fn test_data_crosses_a_relay_reached_over_tcp() {
    let Ok(server) = std::env::var("WEBRTC_TURN_TCP_SERVER") else {
        eprintln!("skipping: set WEBRTC_TURN_TCP_SERVER=host:port (see the module docs)");
        return;
    };
    let credentials =
        std::env::var("WEBRTC_TURN_CREDENTIALS").unwrap_or_else(|_| "user:pass".to_owned());
    let (username, credential) = credentials
        .split_once(':')
        .expect("WEBRTC_TURN_CREDENTIALS=user:pass");
    block_on(run(&server, username, credential)).unwrap();
}

async fn run(server: &str, username: &str, credential: &str) -> Result<()> {
    let runtime = runtime();
    let (off_gather_tx, mut off_gather_rx) = channel::<()>(1);
    let (off_conn_tx, mut off_conn_rx) = channel::<()>(1);
    let (ans_gather_tx, mut ans_gather_rx) = channel::<()>(1);
    let (ans_conn_tx, mut ans_conn_rx) = channel::<()>(1);
    let (message_tx, mut message_rx) = channel::<String>(4);

    let offerer = PeerConnectionBuilder::new()
        .with_configuration(
            relay_only(format!("turn:{server}?transport=tcp"), username, credential).build(),
        )
        .with_handler(Arc::new(Handler {
            gather_tx: off_gather_tx,
            connected_tx: off_conn_tx,
            message_tx: None,
            runtime: runtime.clone(),
        }))
        .with_runtime(runtime.clone())
        .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
        .build()
        .await?;
    let dc = offerer.create_data_channel("tcp-relay", None).await?;
    let (open_tx, mut open_rx) = channel::<()>(1);
    {
        let dc = dc.clone();
        runtime.spawn(Box::pin(async move {
            while let Some(event) = dc.poll().await {
                match event {
                    DataChannelEvent::OnOpen => {
                        let _ = open_tx.try_send(());
                    }
                    DataChannelEvent::OnClose => break,
                    _ => {}
                }
            }
        }));
    }

    let answerer = PeerConnectionBuilder::new()
        .with_configuration(
            relay_only(format!("turn:{server}?transport=udp"), username, credential).build(),
        )
        .with_handler(Arc::new(Handler {
            gather_tx: ans_gather_tx,
            connected_tx: ans_conn_tx,
            message_tx: Some(message_tx),
            runtime: runtime.clone(),
        }))
        .with_runtime(runtime.clone())
        .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
        .build()
        .await?;

    let offer = offerer.create_offer(None).await?;
    offerer.set_local_description(offer).await?;
    timeout(Duration::from_secs(10), off_gather_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("the offerer never finished gathering"))?;
    let offer_sdp = offerer.local_description().await.expect("offer");
    assert!(
        offer_sdp.sdp.contains("typ relay"),
        "the offerer gathered no relay over TCP:\n{}",
        offer_sdp.sdp
    );

    answerer.set_remote_description(offer_sdp).await?;
    let answer = answerer.create_answer(None).await?;
    answerer.set_local_description(answer).await?;
    timeout(Duration::from_secs(10), ans_gather_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("the answerer never finished gathering"))?;
    let answer_sdp = answerer.local_description().await.expect("answer");
    assert!(
        answer_sdp.sdp.contains("typ relay"),
        "the answerer gathered no relay over UDP:\n{}",
        answer_sdp.sdp
    );
    offerer.set_remote_description(answer_sdp).await?;

    timeout(Duration::from_secs(20), off_conn_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("the offerer never connected through the relays"))?;
    timeout(Duration::from_secs(10), ans_conn_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("the answerer never connected through the relays"))?;
    timeout(Duration::from_secs(10), open_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("the data channel never opened"))?;

    dc.send_text(MESSAGE).await?;
    let received = timeout(Duration::from_secs(10), message_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("the message never arrived"))?
        .expect("a message");
    assert_eq!(received, MESSAGE);

    offerer.close().await?;
    answerer.close().await?;
    Ok(())
}
