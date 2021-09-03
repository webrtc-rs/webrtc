use super::*;
use crate::api::media_engine::MIME_TYPE_OPUS;
use crate::media::rtp::rtp_codec::RTPHeaderExtensionParameter;
use crate::media::rtp::RTCPFeedback;
use crate::media::track::track_local::track_local_static_sample::TrackLocalStaticSample;
use crate::media::track::track_local::TrackLocal;
use crate::media::Sample;
use crate::peer::peer_connection::peer_connection_test::{
    close_pair_now, create_vnet_pair, signal_pair, until_connection_state,
};
use crate::peer::peer_connection_state::PeerConnectionState;
use bytes::Bytes;
use tokio::time::Duration;
use waitgroup::WaitGroup;

lazy_static! {
    static ref P: RTPParameters = RTPParameters {
        codecs: vec![RTPCodecParameters {
            capability: RTPCodecCapability {
                mime_type: MIME_TYPE_OPUS.to_string(),
                clock_rate: 48000,
                channels: 2,
                sdp_fmtp_line: "minptime=10;useinbandfec=1".to_string(),
                rtcp_feedback: vec![RTCPFeedback {
                    typ: "nack".to_owned(),
                    parameter: "".to_owned(),
                }],
            },
            payload_type: 111,
            ..Default::default()
        }],
        header_extensions: vec![
            RTPHeaderExtensionParameter {
                uri: "urn:ietf:params:rtp-hdrext:sdes:mid".to_owned(),
                ..Default::default()
            },
            RTPHeaderExtensionParameter {
                uri: "urn:ietf:params:rtp-hdrext:sdes:rtp-stream-id".to_owned(),
                ..Default::default()
            },
            RTPHeaderExtensionParameter {
                uri: "urn:ietf:params:rtp-hdrext:sdes:repaired-rtp-stream-id".to_owned(),
                ..Default::default()
            },
        ],
    };
}

#[tokio::test]
async fn test_set_rtp_parameters() -> Result<()> {
    let (mut sender, mut receiver, wan) = create_vnet_pair().await?;

    let outgoing_track: Arc<dyn TrackLocal + Send + Sync> = Arc::new(TrackLocalStaticSample::new(
        RTPCodecCapability {
            mime_type: "video/vp8".to_owned(),
            ..Default::default()
        },
        "video".to_owned(),
        "webrtc-rs".to_owned(),
    ));

    sender.add_track(Arc::clone(&outgoing_track)).await?;

    // Those parameters wouldn't make sense in a real application,
    // but for the sake of the test we just need different values.

    let (seen_packet_tx, _seen_packet_rx) = mpsc::channel::<()>(1);
    let seen_packet_tx = Arc::new(Mutex::new(Some(seen_packet_tx)));
    receiver
        .on_track(Box::new(
            move |_: Option<Arc<TrackRemote>>, receiver: Option<Arc<RTPReceiver>>| {
                let seen_packet_tx2 = Arc::clone(&seen_packet_tx);
                Box::pin(async move {
                    if let Some(r) = &receiver {
                        r.set_rtp_parameters(P.clone()).await;

                        if let Some(t) = r.track().await {
                            let incoming_track_codecs = t.codec().await;

                            assert_eq!(P.header_extensions, t.params().await.header_extensions);
                            assert_eq!(
                                P.codecs[0].capability.mime_type,
                                incoming_track_codecs.capability.mime_type
                            );
                            assert_eq!(
                                P.codecs[0].capability.clock_rate,
                                incoming_track_codecs.capability.clock_rate
                            );
                            assert_eq!(
                                P.codecs[0].capability.channels,
                                incoming_track_codecs.capability.channels
                            );
                            assert_eq!(
                                P.codecs[0].capability.sdp_fmtp_line,
                                incoming_track_codecs.capability.sdp_fmtp_line
                            );
                            assert_eq!(
                                P.codecs[0].capability.rtcp_feedback,
                                incoming_track_codecs.capability.rtcp_feedback
                            );
                            assert_eq!(
                                P.codecs[0].payload_type,
                                incoming_track_codecs.payload_type
                            );

                            {
                                let mut done = seen_packet_tx2.lock().await;
                                done.take();
                            }
                        }
                    }
                })
            },
        ))
        .await;

    let wg = WaitGroup::new();

    until_connection_state(&mut sender, &wg, PeerConnectionState::Connected).await;
    until_connection_state(&mut receiver, &wg, PeerConnectionState::Connected).await;

    signal_pair(&mut sender, &mut receiver).await?;

    //TODO: wg.wait().await;

    if let Some(v) = outgoing_track
        .as_any()
        .downcast_ref::<TrackLocalStaticSample>()
    {
        v.write_sample(&Sample {
            data: Bytes::from_static(&[0xAA]),
            duration: Duration::from_secs(1),
            ..Default::default()
        })
        .await?;
    } else {
        assert!(false);
    }

    //TODO: let _ = seen_packet_rx.recv().await;
    {
        let mut w = wan.lock().await;
        w.stop().await?;
    }
    close_pair_now(&sender, &receiver).await;

    Ok(())
}

/*TODO:
// Assert that SetReadDeadline works as expected
// This test uses VNet since we must have zero loss
func Test_RTPReceiver_SetReadDeadline()->Result<()> {
    lim := test.TimeOut(time.Second * 30)
    defer lim.Stop()

    report := test.CheckRoutines(t)
    defer report()

    sender, receiver, wan := createVNetPair(t)

    track, err := NewTrackLocalStaticSample(RTPCodecCapability{MimeType: "video/vp8"}, "video", "pion")
    assert.NoError(t, err)

    _, err = sender.AddTrack(track)
    assert.NoError(t, err)

    seenPacket, seenPacketCancel := context.WithCancel(context.Background())
    receiver.OnTrack(func(trackRemote *TrackRemote, r *RTPReceiver) {
        // Set Deadline for both RTP and RTCP Stream
        assert.NoError(t, r.SetReadDeadline(time.Now().Add(time.Second)))
        assert.NoError(t, trackRemote.SetReadDeadline(time.Now().Add(time.Second)))

        // First call will not error because we cache for probing
        _, _, readErr := trackRemote.ReadRTP()
        assert.NoError(t, readErr)

        _, _, readErr = trackRemote.ReadRTP()
        assert.Error(t, readErr, packetio.ErrTimeout)

        _, _, readErr = r.ReadRTCP()
        assert.Error(t, readErr, packetio.ErrTimeout)

        seenPacketCancel()
    })

    peerConnectionsConnected := until_connection_state(PeerConnectionStateConnected, sender, receiver)

    assert.NoError(t, signalPair(sender, receiver))

    peerConnectionsConnected.Wait()
    assert.NoError(t, track.WriteSample(media.Sample{Data: []byte{0xAA}, Duration: time.Second}))

    <-seenPacket.Done()
    assert.NoError(t, wan.Stop())
    closePairNow(t, sender, receiver)
}
*/
