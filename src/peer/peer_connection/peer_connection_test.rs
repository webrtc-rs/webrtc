use super::*;
use crate::media::Sample;
use bytes::Bytes;
use tokio::time::Duration;

/// new_pair creates two new peer connections (an offerer and an answerer)
/// *without* using an api (i.e. using the default settings).
pub(crate) async fn new_pair(api: &API) -> Result<(PeerConnection, PeerConnection)> {
    let pca = api.new_peer_connection(Configuration::default()).await?;
    let pcb = api.new_peer_connection(Configuration::default()).await?;

    Ok((pca, pcb))
}

pub(crate) async fn signal_pair(
    pc_offer: &mut PeerConnection,
    pc_answer: &mut PeerConnection,
) -> Result<()> {
    // Note(albrow): We need to create a data channel in order to trigger ICE
    // candidate gathering in the background for the JavaScript/Wasm bindings. If
    // we don't do this, the complete offer including ICE candidates will never be
    // generated.
    pc_offer
        .create_data_channel("initial_data_channel", None)
        .await?;

    let offer = pc_offer.create_offer(None).await?;

    let mut offer_gathering_complete = pc_offer.gathering_complete_promise().await;
    pc_offer.set_local_description(offer).await?;

    let _ = offer_gathering_complete.recv().await;

    pc_answer
        .set_remote_description(
            pc_offer
                .local_description()
                .await
                .ok_or(Error::new("non local description".to_owned()))?,
        )
        .await?;

    let answer = pc_answer.create_answer(None).await?;

    let mut answer_gathering_complete = pc_answer.gathering_complete_promise().await;
    pc_answer.set_local_description(answer).await?;

    let _ = answer_gathering_complete.recv().await;

    pc_offer
        .set_remote_description(
            pc_answer
                .local_description()
                .await
                .ok_or(Error::new("non local description".to_owned()))?,
        )
        .await
}

pub(crate) async fn close_pair_now(pc1: &PeerConnection, pc2: &PeerConnection) {
    let mut fail = false;
    if let Err(err) = pc1.close().await {
        log::error!("Failed to close PeerConnection: {}", err);
        fail = true;
    }
    if let Err(err) = pc2.close().await {
        log::error!("Failed to close PeerConnection: {}", err);
        fail = true;
    }

    assert!(!fail);
}

pub(crate) async fn close_pair(
    pc1: &PeerConnection,
    pc2: &PeerConnection,
    mut done_rx: mpsc::Receiver<()>,
) {
    let timeout = tokio::time::sleep(Duration::from_secs(1));
    tokio::pin!(timeout);

    tokio::select! {
        _ = timeout.as_mut() =>{
            assert!(false, "close_pair timed out waiting for done signal");
        }
        _ = done_rx.recv() =>{
            close_pair_now(pc1, pc2).await;
        }
    }
}

/*
func offerMediaHasDirection(offer SessionDescription, kind RTPCodecType, direction RTPTransceiverDirection) bool {
    parsed := &sdp.SessionDescription{}
    if err := parsed.Unmarshal([]byte(offer.SDP)); err != nil {
        return false
    }

    for _, media := range parsed.MediaDescriptions {
        if media.MediaName.Media == kind.String() {
            _, exists := media.Attribute(direction.String())
            return exists
        }
    }
    return false
}*/

pub(crate) async fn send_video_until_done(
    mut done_rx: mpsc::Receiver<()>,
    tracks: &[Arc<dyn TrackLocal + Send + Sync>],
) {
    loop {
        let timeout = tokio::time::sleep(Duration::from_millis(20));
        tokio::pin!(timeout);

        tokio::select! {
            _ = timeout.as_mut() =>{
                for track in tracks {
                    if let Some(t) = track.as_any().downcast_ref::<TrackLocalStaticSample>(){
                        assert!(t.write_sample(&Sample{
                            data: Bytes::from_static(&[0x00]),
                            duration: Duration::from_secs(1),
                            ..Default::default()
                        }).await.is_ok());
                    }else{
                        assert!(false);
                    }
                }
            }
            _ = done_rx.recv() =>{
                return;
            }
        }
    }
}
