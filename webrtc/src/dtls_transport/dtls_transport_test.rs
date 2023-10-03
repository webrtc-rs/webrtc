use ice::mdns::MulticastDnsMode;
use ice::network_type::NetworkType;
use regex::Regex;
use tokio::time::Duration;
use waitgroup::WaitGroup;

use super::*;
use crate::api::media_engine::MediaEngine;
use crate::api::APIBuilder;
use crate::data_channel::RTCDataChannel;
use crate::ice_transport::ice_candidate::RTCIceCandidate;
use crate::peer_connection::configuration::RTCConfiguration;
use crate::peer_connection::peer_connection_state::RTCPeerConnectionState;
use crate::peer_connection::peer_connection_test::{
    close_pair_now, new_pair, signal_pair, until_connection_state,
};

//use log::LevelFilter;
//use std::io::Write;

// An invalid fingerprint MUST cause PeerConnectionState to go to PeerConnectionStateFailed
#[tokio::test]
async fn test_invalid_fingerprint_causes_failed() -> Result<()> {
    /*env_logger::Builder::new()
    .format(|buf, record| {
        writeln!(
            buf,
            "{}:{} [{}] {} - {}",
            record.file().unwrap_or("unknown"),
            record.line().unwrap_or(0),
            record.level(),
            chrono::Local::now().format("%H:%M:%S.%6f"),
            record.args()
        )
    })
    .filter(None, LevelFilter::Trace)
    .init();*/

    let mut m = MediaEngine::default();
    m.register_default_codecs()?;
    let api = APIBuilder::new().with_media_engine(m).build();

    let (mut pc_offer, mut pc_answer) = new_pair(&api).await?;

    pc_answer.on_data_channel(Box::new(|_: Arc<RTCDataChannel>| {
        panic!("A DataChannel must not be created when Fingerprint verification fails");
    }));

    let (offer_chan_tx, mut offer_chan_rx) = mpsc::channel::<()>(1);

    let offer_chan_tx = Arc::new(offer_chan_tx);
    pc_offer.on_ice_candidate(Box::new(move |candidate: Option<RTCIceCandidate>| {
        let offer_chan_tx2 = Arc::clone(&offer_chan_tx);
        Box::pin(async move {
            if candidate.is_none() {
                let _ = offer_chan_tx2.send(()).await;
            }
        })
    }));

    let offer_connection_has_failed = WaitGroup::new();
    until_connection_state(
        &mut pc_offer,
        &offer_connection_has_failed,
        RTCPeerConnectionState::Failed,
    )
    .await;
    let answer_connection_has_failed = WaitGroup::new();
    until_connection_state(
        &mut pc_answer,
        &answer_connection_has_failed,
        RTCPeerConnectionState::Failed,
    )
    .await;

    let _ = pc_offer
        .create_data_channel("unusedDataChannel", None)
        .await?;

    let offer = pc_offer.create_offer(None).await?;
    pc_offer.set_local_description(offer).await?;

    let timeout = tokio::time::sleep(Duration::from_secs(1));
    tokio::pin!(timeout);

    tokio::select! {
        _ = offer_chan_rx.recv() =>{
            let mut offer = pc_offer.pending_local_description().await.unwrap();

            log::trace!("receiving pending local desc: {:?}", offer);

            // Replace with invalid fingerprint
            let re = Regex::new(r"sha-256 (.*?)\r").unwrap();
            offer.sdp = re.replace_all(offer.sdp.as_str(), "sha-256 AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA\r").to_string();

            pc_answer.set_remote_description(offer).await?;

            let mut answer = pc_answer.create_answer(None).await?;

            pc_answer.set_local_description(answer.clone()).await?;

            answer.sdp = re.replace_all(answer.sdp.as_str(), "sha-256 AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA\r").to_string();

            pc_offer.set_remote_description(answer).await?;
        }
        _ = timeout.as_mut() =>{
            panic!("timed out waiting to receive offer");
        }
    }

    log::trace!("offer_connection_has_failed wait begin");

    offer_connection_has_failed.wait().await;
    answer_connection_has_failed.wait().await;

    log::trace!("offer_connection_has_failed wait end");
    {
        let transport = pc_offer.sctp().transport();
        assert_eq!(transport.state(), RTCDtlsTransportState::Failed);
        assert!(transport.conn().await.is_none());
    }

    {
        let transport = pc_answer.sctp().transport();
        assert_eq!(transport.state(), RTCDtlsTransportState::Failed);
        assert!(transport.conn().await.is_none());
    }

    close_pair_now(&pc_offer, &pc_answer).await;

    Ok(())
}

async fn run_test(r: DTLSRole) -> Result<()> {
    let mut offer_s = SettingEngine::default();
    offer_s.set_answering_dtls_role(r)?;
    offer_s.set_ice_multicast_dns_mode(MulticastDnsMode::Disabled);
    offer_s.set_network_types(vec![NetworkType::Udp4]);
    let mut offer_pc = APIBuilder::new()
        .with_setting_engine(offer_s)
        .build()
        .new_peer_connection(RTCConfiguration::default())
        .await?;

    let mut answer_s = SettingEngine::default();
    answer_s.set_answering_dtls_role(r)?;
    answer_s.set_ice_multicast_dns_mode(MulticastDnsMode::Disabled);
    answer_s.set_network_types(vec![NetworkType::Udp4]);
    let mut answer_pc = APIBuilder::new()
        .with_setting_engine(answer_s)
        .build()
        .new_peer_connection(RTCConfiguration::default())
        .await?;

    signal_pair(&mut offer_pc, &mut answer_pc).await?;

    let wg = WaitGroup::new();
    until_connection_state(&mut answer_pc, &wg, RTCPeerConnectionState::Connected).await;
    wg.wait().await;

    close_pair_now(&offer_pc, &answer_pc).await;

    Ok(())
}

#[tokio::test]
async fn test_peer_connection_dtls_role_setting_engine_server() -> Result<()> {
    /*env_logger::Builder::new()
    .format(|buf, record| {
        writeln!(
            buf,
            "{}:{} [{}] {} - {}",
            record.file().unwrap_or("unknown"),
            record.line().unwrap_or(0),
            record.level(),
            chrono::Local::now().format("%H:%M:%S.%6f"),
            record.args()
        )
    })
    .filter(None, LevelFilter::Trace)
    .init();*/

    run_test(DTLSRole::Server).await
}

#[tokio::test]
async fn test_peer_connection_dtls_role_setting_engine_client() -> Result<()> {
    /*env_logger::Builder::new()
    .format(|buf, record| {
        writeln!(
            buf,
            "{}:{} [{}] {} - {}",
            record.file().unwrap_or("unknown"),
            record.line().unwrap_or(0),
            record.level(),
            chrono::Local::now().format("%H:%M:%S.%6f"),
            record.args()
        )
    })
    .filter(None, LevelFilter::Trace)
    .init();*/

    run_test(DTLSRole::Client).await
}
