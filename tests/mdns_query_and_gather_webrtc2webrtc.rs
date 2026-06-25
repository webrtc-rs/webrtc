use anyhow::Result;
use rtc::ice::mdns::MulticastDnsMode;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::time::Duration;
use webrtc::data_channel::{DataChannel, DataChannelEvent};
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCIceGatheringState,
    RTCPeerConnectionState, SettingEngine,
};
use webrtc::runtime::{Runtime, Sender, block_on, channel, default_runtime, sleep, timeout};

const TEST_MESSAGE: &str = "hello over mdns";
const ECHO_MESSAGE: &str = "echo over mdns";

struct OffererHandler {
    gather_complete_tx: Sender<()>,
    connected_tx: Sender<()>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for OffererHandler {
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
}

struct AnswererHandler {
    gather_complete_tx: Sender<()>,
    connected_tx: Sender<()>,
    answer_msg_tx: Sender<String>,
    runtime: Arc<dyn Runtime>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for AnswererHandler {
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
        let answer_msg_tx = self.answer_msg_tx.clone();
        self.runtime.spawn(Box::pin(async move {
            while let Some(event) = dc.poll().await {
                match event {
                    DataChannelEvent::OnMessage(msg) => {
                        let data = String::from_utf8(msg.data.to_vec()).unwrap_or_default();
                        answer_msg_tx.try_send(data).ok();
                        let _ = dc.send_text(ECHO_MESSAGE).await;
                    }
                    DataChannelEvent::OnClose => break,
                    _ => {}
                }
            }
        }));
    }
}

fn mdns_setting_engine(name: &str, local_ip: IpAddr) -> SettingEngine {
    let mut setting_engine = SettingEngine::default();
    setting_engine.set_multicast_dns_mode(MulticastDnsMode::QueryAndGather);
    setting_engine.set_multicast_dns_timeout(Some(Duration::from_secs(5)));
    setting_engine.set_multicast_dns_local_name(name.to_owned());
    setting_engine.set_multicast_dns_local_ip(Some(local_ip));
    setting_engine
}

#[test]
fn test_mdns_query_and_gather_webrtc_to_webrtc() {
    block_on(run_test()).unwrap();
}

async fn run_test() -> Result<()> {
    env_logger::builder().is_test(true).try_init().ok();

    let runtime =
        default_runtime().ok_or_else(|| std::io::Error::other("no async runtime found"))?;

    let (offerer_gather_tx, mut offerer_gather_rx) = channel::<()>(1);
    let (offerer_connected_tx, mut offerer_connected_rx) = channel::<()>(1);
    let (offerer_dc_open_tx, mut offerer_dc_open_rx) = channel::<()>(8);
    let (offerer_msg_tx, mut offerer_msg_rx) = channel::<String>(32);
    let (answerer_gather_tx, mut answerer_gather_rx) = channel::<()>(1);
    let (answerer_connected_tx, mut answerer_connected_rx) = channel::<()>(1);
    let (answerer_msg_tx, mut answerer_msg_rx) = channel::<String>(32);

    let offerer_pc = PeerConnectionBuilder::new()
        .with_handler(Arc::new(OffererHandler {
            gather_complete_tx: offerer_gather_tx,
            connected_tx: offerer_connected_tx,
        }))
        .with_setting_engine(mdns_setting_engine(
            "offerer-mdns.local",
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        ))
        .with_runtime(runtime.clone())
        .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
        .build()
        .await?;

    let offerer_dc = offerer_pc.create_data_channel("mdns", None).await?;
    {
        let dc = offerer_dc.clone();
        let dc_open_tx = offerer_dc_open_tx.clone();
        let msg_tx = offerer_msg_tx.clone();
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
                    DataChannelEvent::OnClose => break,
                    _ => {}
                }
            }
        }));
    }

    let offer = offerer_pc.create_offer(None).await?;
    offerer_pc.set_local_description(offer).await?;
    let _ = timeout(Duration::from_secs(5), offerer_gather_rx.recv()).await;
    let offer_sdp = offerer_pc
        .local_description()
        .await
        .expect("offerer local description should be set");

    let answerer_pc = PeerConnectionBuilder::new()
        .with_handler(Arc::new(AnswererHandler {
            gather_complete_tx: answerer_gather_tx,
            connected_tx: answerer_connected_tx,
            answer_msg_tx: answerer_msg_tx,
            runtime: runtime.clone(),
        }))
        .with_setting_engine(mdns_setting_engine(
            "answerer-mdns.local",
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        ))
        .with_runtime(runtime.clone())
        .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
        .build()
        .await?;

    answerer_pc.set_remote_description(offer_sdp).await?;
    let answer = answerer_pc.create_answer(None).await?;
    answerer_pc.set_local_description(answer).await?;
    let _ = timeout(Duration::from_secs(5), answerer_gather_rx.recv()).await;
    let answer_sdp = answerer_pc
        .local_description()
        .await
        .expect("answerer local description should be set");

    answer_sdp
        .sdp
        .lines()
        .find(|line| line.starts_with("a=candidate:") && line.contains(".local"))
        .expect("expected answer SDP to include an mDNS candidate");

    offerer_pc.set_remote_description(answer_sdp).await?;

    timeout(Duration::from_secs(15), offerer_connected_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("Timeout waiting for offerer to connect"))?;
    timeout(Duration::from_secs(15), answerer_connected_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("Timeout waiting for answerer to connect"))?;
    timeout(Duration::from_secs(10), offerer_dc_open_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("Timeout waiting for data channel to open"))?;

    offerer_dc.send_text(TEST_MESSAGE).await?;

    let answer_msg = timeout(Duration::from_secs(10), answerer_msg_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("Timeout waiting for answerer message"))?
        .ok_or_else(|| anyhow::anyhow!("Answerer message channel closed"))?;
    let echoed = timeout(Duration::from_secs(10), offerer_msg_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("Timeout waiting for echoed message"))?
        .ok_or_else(|| anyhow::anyhow!("Offerer message channel closed"))?;

    assert_eq!(answer_msg, TEST_MESSAGE);
    assert_eq!(echoed, ECHO_MESSAGE);

    sleep(Duration::from_millis(100)).await;
    offerer_pc.close().await?;
    answerer_pc.close().await?;

    Ok(())
}
