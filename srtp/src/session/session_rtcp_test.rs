#[cfg(test)]
mod session_rtcp_test {
    use crate::{
        config, context::Context, protection_profile::ProtectionProfile, session::Session,
        stream::Stream,
    };

    use std::io::{BufReader, BufWriter};

    use util::Error;

    use std::sync::Arc;
    use tokio::{
        net::UdpSocket,
        sync::{mpsc, Mutex},
    };

    async fn build_session_srtcp_pair() -> Result<(Session, Session), Error> {
        let ua = UdpSocket::bind("127.0.0.1:0").await?;
        let ub = UdpSocket::bind("127.0.0.1:0").await?;

        ua.connect(ub.local_addr()?).await?;
        ub.connect(ua.local_addr()?).await?;

        let ca = config::Config {
            profile: ProtectionProfile::AES128CMHMACSHA1_80,
            keys: config::SessionKeys {
                local_master_key: vec![
                    0xE1, 0xF9, 0x7A, 0x0D, 0x3E, 0x01, 0x8B, 0xE0, 0xD6, 0x4F, 0xA3, 0x2C, 0x06,
                    0xDE, 0x41, 0x39,
                ],
                local_master_salt: vec![
                    0x0E, 0xC6, 0x75, 0xAD, 0x49, 0x8A, 0xFE, 0xEB, 0xB6, 0x96, 0x0B, 0x3A, 0xAB,
                    0xE6,
                ],
                remote_master_key: vec![
                    0xE1, 0xF9, 0x7A, 0x0D, 0x3E, 0x01, 0x8B, 0xE0, 0xD6, 0x4F, 0xA3, 0x2C, 0x06,
                    0xDE, 0x41, 0x39,
                ],
                remote_master_salt: vec![
                    0x0E, 0xC6, 0x75, 0xAD, 0x49, 0x8A, 0xFE, 0xEB, 0xB6, 0x96, 0x0B, 0x3A, 0xAB,
                    0xE6,
                ],
            },

            local_rtp_options: None,
            remote_rtp_options: None,

            local_rtcp_options: None,
            remote_rtcp_options: None,
        };

        let cb = config::Config {
            profile: ProtectionProfile::AES128CMHMACSHA1_80,
            keys: config::SessionKeys {
                local_master_key: vec![
                    0xE1, 0xF9, 0x7A, 0x0D, 0x3E, 0x01, 0x8B, 0xE0, 0xD6, 0x4F, 0xA3, 0x2C, 0x06,
                    0xDE, 0x41, 0x39,
                ],
                local_master_salt: vec![
                    0x0E, 0xC6, 0x75, 0xAD, 0x49, 0x8A, 0xFE, 0xEB, 0xB6, 0x96, 0x0B, 0x3A, 0xAB,
                    0xE6,
                ],
                remote_master_key: vec![
                    0xE1, 0xF9, 0x7A, 0x0D, 0x3E, 0x01, 0x8B, 0xE0, 0xD6, 0x4F, 0xA3, 0x2C, 0x06,
                    0xDE, 0x41, 0x39,
                ],
                remote_master_salt: vec![
                    0x0E, 0xC6, 0x75, 0xAD, 0x49, 0x8A, 0xFE, 0xEB, 0xB6, 0x96, 0x0B, 0x3A, 0xAB,
                    0xE6,
                ],
            },

            local_rtp_options: None,
            remote_rtp_options: None,

            local_rtcp_options: None,
            remote_rtcp_options: None,
        };

        let sa = Session::new(ua, ca, false).await?;
        let sb = Session::new(ub, cb, false).await?;

        Ok((sa, sb))
    }

    const TEST_SSRC: u32 = 5000;

    #[tokio::test]
    async fn test_session_srtcp_accept() -> Result<(), Error> {
        let (mut sa, mut sb) = build_session_srtcp_pair().await?;

        let rtcp_packet = rtcp::packet::Packet::PictureLossIndication(
            rtcp::picture_loss_indication::PictureLossIndication {
                media_ssrc: TEST_SSRC,
                ..Default::default()
            },
        );

        let mut test_payload = vec![];
        {
            let mut writer = BufWriter::<&mut Vec<u8>>::new(test_payload.as_mut());
            rtcp_packet.marshal(&mut writer)?;
        }

        let mut read_buffer = vec![0; test_payload.len()];

        sa.write_rtcp(&rtcp_packet).await?;

        let mut read_stream = sb.accept().await?;
        let ssrc = read_stream.get_ssrc();
        assert_eq!(
            ssrc, TEST_SSRC,
            "SSRC mismatch during accept exp({}) actual({})",
            TEST_SSRC, ssrc
        );

        read_stream.read(&mut read_buffer).await?;

        assert_eq!(
            &test_payload[..],
            &read_buffer[..],
            "Sent buffer does not match the one received exp({:?}) actual({:?})",
            &test_payload[..],
            &read_buffer[..]
        );

        sa.close().await?;
        sb.close().await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_session_srtcp_listen() -> Result<(), Error> {
        let (mut sa, mut sb) = build_session_srtcp_pair().await?;

        let rtcp_packet = rtcp::packet::Packet::PictureLossIndication(
            rtcp::picture_loss_indication::PictureLossIndication {
                media_ssrc: TEST_SSRC,
                ..Default::default()
            },
        );

        let mut test_payload = vec![];
        {
            let mut writer = BufWriter::<&mut Vec<u8>>::new(test_payload.as_mut());
            rtcp_packet.marshal(&mut writer)?;
        }

        let mut read_buffer = vec![0; test_payload.len()];

        let mut read_stream = sb.listen(TEST_SSRC).await?;

        sa.write_rtcp(&rtcp_packet).await?;

        read_stream.read(&mut read_buffer).await?;

        assert_eq!(
            &test_payload[..],
            &read_buffer[..],
            "Sent buffer does not match the one received exp({:?}) actual({:?})",
            &test_payload[..],
            &read_buffer[..]
        );

        sa.close().await?;
        sb.close().await?;

        Ok(())
    }

    fn encrypt_srtcp(context: &mut Context, pkt: &rtcp::packet::Packet) -> Result<Vec<u8>, Error> {
        let mut decrypted = vec![];
        {
            let mut writer = BufWriter::<&mut Vec<u8>>::new(decrypted.as_mut());
            pkt.marshal(&mut writer)?;
        }

        let encrypted = context.encrypt_rtcp(&decrypted)?;

        Ok(encrypted)
    }

    const PLI_PACKET_SIZE: usize = 8;

    async fn get_sender_ssrc(read_stream: &mut Stream) -> Result<u32, Error> {
        let auth_tag_size = ProtectionProfile::AES128CMHMACSHA1_80.auth_tag_len()?;
        let mut read_buffer = vec![0; PLI_PACKET_SIZE + auth_tag_size];

        let (n, _) = read_stream.read_rtcp(&mut read_buffer).await?;

        let mut reader = BufReader::new(&read_buffer[0..n]);
        let pli = rtcp::picture_loss_indication::PictureLossIndication::unmarshal(&mut reader)?;

        Ok(pli.sender_ssrc)
    }

    #[tokio::test]
    async fn test_session_srtcp_replay_protection() -> Result<(), Error> {
        let (mut sa, mut sb) = build_session_srtcp_pair().await?;

        let mut read_stream = sb.listen(TEST_SSRC).await?;

        // Generate test packets
        let mut packets = vec![];
        let mut expected_ssrc = vec![];
        {
            let mut local_context = sa.local_context.lock().await;
            for i in 0..0x10u32 {
                expected_ssrc.push(i);

                let packet = rtcp::packet::Packet::PictureLossIndication(
                    rtcp::picture_loss_indication::PictureLossIndication {
                        media_ssrc: TEST_SSRC,
                        sender_ssrc: i,
                    },
                );

                let encrypted = encrypt_srtcp(&mut local_context, &packet)?;

                packets.push(encrypted);
            }
        }

        let (done_tx, mut done_rx) = mpsc::channel::<()>(1);

        let received_ssrc = Arc::new(Mutex::new(vec![]));
        let cloned_received_ssrc = Arc::clone(&received_ssrc);
        let count = expected_ssrc.len();

        tokio::spawn(async move {
            let mut i = 0;
            while i < count {
                match get_sender_ssrc(&mut read_stream).await {
                    Ok(ssrc) => {
                        let mut r = cloned_received_ssrc.lock().await;
                        r.push(ssrc);

                        i += 1;
                    }
                    Err(_) => break,
                }
            }

            drop(done_tx);
        });

        // Write with replay attack
        for packet in &packets {
            sa.udp_tx.send(packet).await?;

            // Immediately replay
            sa.udp_tx.send(packet).await?;
        }
        for packet in &packets {
            // Delayed replay
            sa.udp_tx.send(packet).await?;
        }

        done_rx.recv().await;

        sa.close().await?;
        sb.close().await?;

        {
            let received_ssrc = received_ssrc.lock().await;
            assert_eq!(&expected_ssrc[..], &received_ssrc[..]);
        }

        Ok(())
    }
}
