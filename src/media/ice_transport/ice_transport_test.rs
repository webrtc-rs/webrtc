use super::*;
use crate::api::media_engine::MediaEngine;
use crate::api::APIBuilder;
use crate::peer::ice::ice_connection_state::ICEConnectionState;
use crate::peer::peer_connection::peer_connection_test::{close_pair_now, new_pair, signal_pair};
use std::sync::atomic::AtomicU32;
use tokio::time::Duration;

#[tokio::test]
async fn test_ice_transport_on_selected_candidate_pair_change() -> Result<()> {
    let mut m = MediaEngine::default();
    m.register_default_codecs()?;
    let api = APIBuilder::new().with_media_engine(m).build();

    let (mut pc_offer, mut pc_answer) = new_pair(&api).await?;

    let (ice_complete_tx, _ice_complete_rx) = mpsc::channel::<()>(1);
    let ice_complete_tx = Arc::new(Mutex::new(Some(ice_complete_tx)));
    pc_answer
        .on_ice_connection_state_change(Box::new(move |ice_state: ICEConnectionState| {
            let ice_complete_tx2 = Arc::clone(&ice_complete_tx);
            Box::pin(async move {
                if ice_state == ICEConnectionState::Connected {
                    tokio::time::sleep(Duration::from_secs(1)).await; //TODO: why sleep 1s?
                    let mut done = ice_complete_tx2.lock().await;
                    done.take();
                }
            })
        }))
        .await;

    let sender_called_candidate_change = Arc::new(AtomicU32::new(0));
    let sender_called_candidate_change2 = Arc::clone(&sender_called_candidate_change);
    pc_offer
        .sctp()
        .transport()
        .ice_transport()
        .on_selected_candidate_pair_change(Box::new(move |_: ICECandidatePair| {
            sender_called_candidate_change2.store(1, Ordering::SeqCst);
            Box::pin(async {})
        }))
        .await;

    signal_pair(&mut pc_offer, &mut pc_answer).await?;

    /*TODO: let _ = ice_complete_rx.recv().await;
    assert_eq!(
        sender_called_candidate_change.load(Ordering::SeqCst),
        1,
        "Sender ICETransport OnSelectedCandidateChange was never called"
    );*/

    close_pair_now(&pc_offer, &pc_answer).await;

    Ok(())
}

#[tokio::test]
async fn test_ice_transport_get_selected_candidate_pair() -> Result<()> {
    /*
    offerer, answerer, err := newPair()
    assert.NoError(t, err)

    peerConnectionConnected := untilConnectionState(PeerConnectionStateConnected, offerer, answerer)

    offererSelectedPair, err := offerer.SCTP().Transport().ICETransport().GetSelectedCandidatePair()
    assert.NoError(t, err)
    assert.Nil(t, offererSelectedPair)

    answererSelectedPair, err := answerer.SCTP().Transport().ICETransport().GetSelectedCandidatePair()
    assert.NoError(t, err)
    assert.Nil(t, answererSelectedPair)

    assert.NoError(t, signalPair(offerer, answerer))
    peerConnectionConnected.Wait()

    offererSelectedPair, err = offerer.SCTP().Transport().ICETransport().GetSelectedCandidatePair()
    assert.NoError(t, err)
    assert.NotNil(t, offererSelectedPair)

    answererSelectedPair, err = answerer.SCTP().Transport().ICETransport().GetSelectedCandidatePair()
    assert.NoError(t, err)
    assert.NotNil(t, answererSelectedPair)

    closePairNow(t, offerer, answerer)
     */
    Ok(())
}
