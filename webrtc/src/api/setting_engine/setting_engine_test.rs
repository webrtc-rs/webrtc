use super::*;
use crate::api::media_engine::MediaEngine;
use crate::api::APIBuilder;
use crate::peer_connection::peer_connection_test::*;
use crate::rtp_transceiver::rtp_codec::RTPCodecType;
use std::sync::atomic::Ordering;

#[test]
fn test_set_connection_timeout() -> Result<()> {
    let mut s = SettingEngine::default();

    assert_eq!(s.timeout.ice_disconnected_timeout, None);
    assert_eq!(s.timeout.ice_failed_timeout, None);
    assert_eq!(s.timeout.ice_keepalive_interval, None);

    s.set_ice_timeouts(
        Some(Duration::from_secs(1)),
        Some(Duration::from_secs(2)),
        Some(Duration::from_secs(3)),
    );
    assert_eq!(
        s.timeout.ice_disconnected_timeout,
        Some(Duration::from_secs(1))
    );
    assert_eq!(s.timeout.ice_failed_timeout, Some(Duration::from_secs(2)));
    assert_eq!(
        s.timeout.ice_keepalive_interval,
        Some(Duration::from_secs(3))
    );

    Ok(())
}

#[test]
fn test_detach_data_channels() -> Result<()> {
    let mut s = SettingEngine::default();

    assert!(
        !s.detach.data_channels,
        "SettingEngine defaults aren't as expected."
    );

    s.detach_data_channels();

    assert!(
        s.detach.data_channels,
        "Failed to enable detached data channels."
    );

    Ok(())
}

#[test]
fn test_set_nat_1to1_ips() -> Result<()> {
    let mut s = SettingEngine::default();

    assert!(
        s.candidates.nat_1to1_ips.is_empty(),
        "Invalid default value"
    );
    assert!(
        s.candidates.nat_1to1_ip_candidate_type == RTCIceCandidateType::Unspecified,
        "Invalid default value"
    );

    let ips = vec!["1.2.3.4".to_owned()];
    let typ = RTCIceCandidateType::Host;
    s.set_nat_1to1_ips(ips, typ);
    assert!(
        !(s.candidates.nat_1to1_ips.len() != 1 || s.candidates.nat_1to1_ips[0] != "1.2.3.4"),
        "Failed to set NAT1To1IPs"
    );
    assert!(
        s.candidates.nat_1to1_ip_candidate_type == typ,
        "Failed to set NAT1To1IPCandidateType"
    );

    Ok(())
}

#[test]
fn test_set_answering_dtls_role() -> Result<()> {
    let mut s = SettingEngine::default();
    assert!(
        s.set_answering_dtls_role(DTLSRole::Auto).is_err(),
        "SetAnsweringDTLSRole can only be called with DTLSRoleClient or DTLSRoleServer"
    );
    assert!(
        s.set_answering_dtls_role(DTLSRole::Unspecified).is_err(),
        "SetAnsweringDTLSRole can only be called with DTLSRoleClient or DTLSRoleServer"
    );

    Ok(())
}

#[test]
fn test_set_replay_protection() -> Result<()> {
    let mut s = SettingEngine::default();

    assert!(
        !(s.replay_protection.dtls != 0
            || s.replay_protection.srtp != 0
            || s.replay_protection.srtcp != 0),
        "SettingEngine defaults aren't as expected."
    );

    s.set_dtls_replay_protection_window(128);
    s.set_srtp_replay_protection_window(64);
    s.set_srtcp_replay_protection_window(32);

    assert!(
        !(s.replay_protection.dtls == 0 || s.replay_protection.dtls != 128),
        "Failed to set DTLS replay protection window"
    );
    assert!(
        !(s.replay_protection.srtp == 0 || s.replay_protection.srtp != 64),
        "Failed to set SRTP replay protection window"
    );
    assert!(
        !(s.replay_protection.srtcp == 0 || s.replay_protection.srtcp != 32),
        "Failed to set SRTCP replay protection window"
    );

    Ok(())
}

/*TODO:#[test] fn test_setting_engine_set_ice_tcp_mux() ->Result<()> {

    listener, err := net.ListenTCP("tcp", &net.TCPAddr{})
    if err != nil {
        panic(err)
    }

    defer func() {
        _ = listener.Close()
    }()

    tcpMux := NewICETCPMux(nil, listener, 8)

    defer func() {
        _ = tcpMux.Close()
    }()

     let mut s = SettingEngine::default();
    settingEngine.SetICETCPMux(tcpMux)

    assert.Equal(t, tcpMux, settingEngine.iceTCPMux)

    Ok(())
}
*/

#[tokio::test]
async fn test_setting_engine_set_disable_media_engine_copy() -> Result<()> {
    //"Copy"
    {
        let mut m = MediaEngine::default();
        m.register_default_codecs()?;
        let api = APIBuilder::new().with_media_engine(m).build();

        let (mut offerer, mut answerer) = new_pair(&api).await?;

        offerer
            .add_transceiver_from_kind(RTPCodecType::Video, None)
            .await?;

        signal_pair(&mut offerer, &mut answerer).await?;

        // Assert that the MediaEngine the user created isn't modified
        assert!(!api.media_engine.negotiated_video.load(Ordering::SeqCst));
        {
            let negotiated_video_codecs = api.media_engine.negotiated_video_codecs.lock();
            assert!(negotiated_video_codecs.is_empty());
        }

        // Assert that the internal MediaEngine is modified
        assert!(offerer
            .internal
            .media_engine
            .negotiated_video
            .load(Ordering::SeqCst));
        {
            let negotiated_video_codecs =
                offerer.internal.media_engine.negotiated_video_codecs.lock();
            assert!(!negotiated_video_codecs.is_empty());
        }

        close_pair_now(&offerer, &answerer).await;

        let (new_offerer, new_answerer) = new_pair(&api).await?;

        // Assert that the first internal MediaEngine hasn't been cleared
        assert!(offerer
            .internal
            .media_engine
            .negotiated_video
            .load(Ordering::SeqCst));
        {
            let negotiated_video_codecs =
                offerer.internal.media_engine.negotiated_video_codecs.lock();
            assert!(!negotiated_video_codecs.is_empty());
        }

        // Assert that the new internal MediaEngine isn't modified
        assert!(!new_offerer
            .internal
            .media_engine
            .negotiated_video
            .load(Ordering::SeqCst));
        {
            let negotiated_video_codecs = new_offerer
                .internal
                .media_engine
                .negotiated_video_codecs
                .lock();
            assert!(negotiated_video_codecs.is_empty());
        }

        close_pair_now(&new_offerer, &new_answerer).await;
    }

    //"No Copy"
    {
        let mut m = MediaEngine::default();
        m.register_default_codecs()?;

        let mut s = SettingEngine::default();
        s.disable_media_engine_copy(true);

        let api = APIBuilder::new()
            .with_media_engine(m)
            .with_setting_engine(s)
            .build();

        let (mut offerer, mut answerer) = new_pair(&api).await?;

        offerer
            .add_transceiver_from_kind(RTPCodecType::Video, None)
            .await?;

        signal_pair(&mut offerer, &mut answerer).await?;

        // Assert that the user MediaEngine was modified, so no copy happened
        assert!(api.media_engine.negotiated_video.load(Ordering::SeqCst));
        {
            let negotiated_video_codecs = api.media_engine.negotiated_video_codecs.lock();
            assert!(!negotiated_video_codecs.is_empty());
        }

        close_pair_now(&offerer, &answerer).await;

        let (offerer, answerer) = new_pair(&api).await?;

        // Assert that the new internal MediaEngine was modified, so no copy happened
        assert!(offerer
            .internal
            .media_engine
            .negotiated_video
            .load(Ordering::SeqCst));
        {
            let negotiated_video_codecs =
                offerer.internal.media_engine.negotiated_video_codecs.lock();
            assert!(!negotiated_video_codecs.is_empty());
        }

        close_pair_now(&offerer, &answerer).await;
    }

    Ok(())
}
