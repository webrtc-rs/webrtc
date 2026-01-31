use portable_atomic::AtomicU32;
use tokio::time::Duration;
use waitgroup::WaitGroup;

use super::*;
use crate::api::media_engine::MediaEngine;
use crate::api::APIBuilder;
use crate::error::Result;
use crate::ice_transport::ice_connection_state::RTCIceConnectionState;
use crate::peer_connection::peer_connection_state::RTCPeerConnectionState;
use crate::peer_connection::peer_connection_test::{
    close_pair_now, new_pair, signal_pair, until_connection_state,
};

#[tokio::test]
async fn test_ice_transport_on_selected_candidate_pair_change() -> Result<()> {
    let mut m = MediaEngine::default();
    m.register_default_codecs()?;
    let api = APIBuilder::new().with_media_engine(m).build();

    let (mut pc_offer, mut pc_answer) = new_pair(&api).await?;

    let (ice_complete_tx, mut ice_complete_rx) = mpsc::channel::<()>(1);
    let ice_complete_tx = Arc::new(Mutex::new(Some(ice_complete_tx)));
    pc_answer.on_ice_connection_state_change(Box::new(move |ice_state: RTCIceConnectionState| {
        let ice_complete_tx2 = Arc::clone(&ice_complete_tx);
        Box::pin(async move {
            if ice_state == RTCIceConnectionState::Connected {
                tokio::time::sleep(Duration::from_secs(1)).await;
                let mut done = ice_complete_tx2.lock().await;
                done.take();
            }
        })
    }));

    let sender_called_candidate_change = Arc::new(AtomicU32::new(0));
    let sender_called_candidate_change2 = Arc::clone(&sender_called_candidate_change);
    pc_offer
        .sctp()
        .transport()
        .ice_transport()
        .on_selected_candidate_pair_change(Box::new(move |_: RTCIceCandidatePair| {
            sender_called_candidate_change2.store(1, Ordering::SeqCst);
            Box::pin(async {})
        }));

    signal_pair(&mut pc_offer, &mut pc_answer).await?;

    let _ = ice_complete_rx.recv().await;
    assert_eq!(
        sender_called_candidate_change.load(Ordering::SeqCst),
        1,
        "Sender ICETransport OnSelectedCandidateChange was never called"
    );

    close_pair_now(&pc_offer, &pc_answer).await;

    Ok(())
}

#[tokio::test]
async fn test_ice_transport_get_selected_candidate_pair() -> Result<()> {
    let mut m = MediaEngine::default();
    m.register_default_codecs()?;
    let api = APIBuilder::new().with_media_engine(m).build();

    let (mut offerer, mut answerer) = new_pair(&api).await?;

    let peer_connection_connected = WaitGroup::new();
    until_connection_state(
        &mut offerer,
        &peer_connection_connected,
        RTCPeerConnectionState::Connected,
    )
    .await;
    until_connection_state(
        &mut answerer,
        &peer_connection_connected,
        RTCPeerConnectionState::Connected,
    )
    .await;

    let offerer_selected_pair = offerer
        .sctp()
        .transport()
        .ice_transport()
        .get_selected_candidate_pair()
        .await;
    assert!(offerer_selected_pair.is_none());

    let answerer_selected_pair = answerer
        .sctp()
        .transport()
        .ice_transport()
        .get_selected_candidate_pair()
        .await;
    assert!(answerer_selected_pair.is_none());

    signal_pair(&mut offerer, &mut answerer).await?;

    peer_connection_connected.wait().await;

    let offerer_selected_pair = offerer
        .sctp()
        .transport()
        .ice_transport()
        .get_selected_candidate_pair()
        .await;
    assert!(offerer_selected_pair.is_some());

    let answerer_selected_pair = answerer
        .sctp()
        .transport()
        .ice_transport()
        .get_selected_candidate_pair()
        .await;
    assert!(answerer_selected_pair.is_some());

    close_pair_now(&offerer, &answerer).await;

    Ok(())
}
