use super::*;
//use crate::api::media_engine::MediaEngine;
//use crate::api::APIBuilder;
//use crate::media::rtp::rtp_codec::RTPCodecType;
//use crate::peer::peer_connection::peer_connection_test::*;
//use std::sync::atomic::Ordering;

#[test]
fn test_set_ephemeral_udpport_range() -> Result<()> {
    let mut s = SettingEngine::default();

    assert!(
        !(s.ephemeral_udp.port_min != 0 || s.ephemeral_udp.port_max != 0),
        "SettingEngine defaults aren't as expected."
    );

    // set bad ephemeral ports
    assert!(
        s.set_ephemeral_udp_port_range(3000, 2999).is_err(),
        "Setting engine should fail bad ephemeral ports."
    );

    assert!(
        s.set_ephemeral_udp_port_range(3000, 4000).is_ok(),
        "Setting engine failed valid port range"
    );

    assert!(
        !(s.ephemeral_udp.port_min != 3000 || s.ephemeral_udp.port_max != 4000),
        "Setting engine ports do not reflect expected range"
    );

    Ok(())
}

#[test]
fn test_set_connection_timeout() -> Result<()> {
    let mut s = SettingEngine::default();

    let d = Duration::default();
    assert_eq!(s.timeout.ice_disconnected_timeout, d);
    assert_eq!(s.timeout.ice_failed_timeout, d);
    assert_eq!(s.timeout.ice_keepalive_interval, d);

    s.set_ice_timeouts(
        Duration::from_secs(1),
        Duration::from_secs(2),
        Duration::from_secs(3),
    );
    assert_eq!(s.timeout.ice_disconnected_timeout, Duration::from_secs(1));
    assert_eq!(s.timeout.ice_failed_timeout, Duration::from_secs(2));
    assert_eq!(s.timeout.ice_keepalive_interval, Duration::from_secs(3));

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
        s.candidates.nat_1to1_ip_candidate_type == ICECandidateType::Unspecified,
        "Invalid default value"
    );

    let ips = vec!["1.2.3.4".to_owned()];
    let typ = ICECandidateType::Host;
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

/*TODO:#[test] fn  TestSettingEngine_SetICETCP()->Result<()> {
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

/*
TODO: missing pc.ops.Enqueue in signal_pair
#[tokio::test]
async fn test_setting_engine_set_disable_media_engine_copy() -> Result<()> {
    //"Copy"
    {
        let (mut offerer, mut answerer) = new_pair().await?;

        offerer
            .add_transceiver_from_kind(RTPCodecType::Video, &[])
            .await?;

        signal_pair(&mut offerer, &mut answerer).await?;

        // Assert that the MediaEngine the user created isn't modified
        /*assert!(!m.negotiated_video.load(Ordering::SeqCst));
        {
            let negotiated_video_codecs = m.negotiated_video_codecs.lock().await;
            assert!(negotiated_video_codecs.is_empty());
        }*/

        // Assert that the internal MediaEngine is modified
        assert!(offerer.media_engine.negotiated_video.load(Ordering::SeqCst));
        {
            let negotiated_video_codecs = offerer.media_engine.negotiated_video_codecs.lock().await;
            assert!(!negotiated_video_codecs.is_empty());
        }

        /*
        closePairNow(t, offerer, answerer)

        newOfferer, newAnswerer, err := api.newPair(Configuration{})
        assert.NoError(t, err)

        // Assert that the first internal MediaEngine hasn't been cleared
        assert.True(t, offerer.api.mediaEngine.negotiatedVideo)
        assert.NotEmpty(t, offerer.api.mediaEngine.negotiatedVideoCodecs)

        // Assert that the new internal MediaEngine isn't modified
        assert.False(t, newOfferer.api.mediaEngine.negotiatedVideo)
        assert.Empty(t, newAnswerer.api.mediaEngine.negotiatedVideoCodecs)

        closePairNow(t, newOfferer, newAnswerer)*/
    }
    /*
        //"No Copy"
        {
            m := &MediaEngine{}
            assert.NoError(t, m.RegisterDefaultCodecs())

            s := SettingEngine{}
            s.DisableMediaEngineCopy(true)

            api := NewAPI(WithMediaEngine(m), WithSettingEngine(s))

            offerer, answerer, err := api.newPair(Configuration{})
            assert.NoError(t, err)

            _, err = offerer.AddTransceiverFromKind(RTPCodecTypeVideo)
            assert.NoError(t, err)

            assert.NoError(t, signalPair(offerer, answerer))

            // Assert that the user MediaEngine was modified, so no copy happened
            assert.True(t, m.negotiatedVideo)
            assert.NotEmpty(t, m.negotiatedVideoCodecs)

            closePairNow(t, offerer, answerer)

            offerer, answerer, err = api.newPair(Configuration{})
            assert.NoError(t, err)

            // Assert that the new internal MediaEngine was modified, so no copy happened
            assert.True(t, offerer.api.mediaEngine.negotiatedVideo)
            assert.NotEmpty(t, offerer.api.mediaEngine.negotiatedVideoCodecs)

            closePairNow(t, offerer, answerer)
        }
    */
    Ok(())
}
*/
