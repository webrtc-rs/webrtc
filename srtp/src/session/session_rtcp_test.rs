use std::sync::Arc;

use bytes::{Bytes, BytesMut};
use rtcp::payload_feedbacks::*;
use tokio::sync::{mpsc, Mutex};
use util::conn::conn_pipe::*;

use super::*;
use crate::error::Result;
use crate::protection_profile::*;

async fn build_session_srtcp_pair() -> Result<(Session, Session)> {
    let (ua, ub) = pipe();

    let ca = Config {
        profile: ProtectionProfile::Aes128CmHmacSha1_80,
        keys: SessionKeys {
            local_master_key: vec![
                0xE1, 0xF9, 0x7A, 0x0D, 0x3E, 0x01, 0x8B, 0xE0, 0xD6, 0x4F, 0xA3, 0x2C, 0x06, 0xDE,
                0x41, 0x39,
            ],
            local_master_salt: vec![
                0x0E, 0xC6, 0x75, 0xAD, 0x49, 0x8A, 0xFE, 0xEB, 0xB6, 0x96, 0x0B, 0x3A, 0xAB, 0xE6,
            ],
            remote_master_key: vec![
                0xE1, 0xF9, 0x7A, 0x0D, 0x3E, 0x01, 0x8B, 0xE0, 0xD6, 0x4F, 0xA3, 0x2C, 0x06, 0xDE,
                0x41, 0x39,
            ],
            remote_master_salt: vec![
                0x0E, 0xC6, 0x75, 0xAD, 0x49, 0x8A, 0xFE, 0xEB, 0xB6, 0x96, 0x0B, 0x3A, 0xAB, 0xE6,
            ],
        },

        local_rtp_options: None,
        remote_rtp_options: None,

        local_rtcp_options: None,
        remote_rtcp_options: None,
    };

    let cb = Config {
        profile: ProtectionProfile::Aes128CmHmacSha1_80,
        keys: SessionKeys {
            local_master_key: vec![
                0xE1, 0xF9, 0x7A, 0x0D, 0x3E, 0x01, 0x8B, 0xE0, 0xD6, 0x4F, 0xA3, 0x2C, 0x06, 0xDE,
                0x41, 0x39,
            ],
            local_master_salt: vec![
                0x0E, 0xC6, 0x75, 0xAD, 0x49, 0x8A, 0xFE, 0xEB, 0xB6, 0x96, 0x0B, 0x3A, 0xAB, 0xE6,
            ],
            remote_master_key: vec![
                0xE1, 0xF9, 0x7A, 0x0D, 0x3E, 0x01, 0x8B, 0xE0, 0xD6, 0x4F, 0xA3, 0x2C, 0x06, 0xDE,
                0x41, 0x39,
            ],
            remote_master_salt: vec![
                0x0E, 0xC6, 0x75, 0xAD, 0x49, 0x8A, 0xFE, 0xEB, 0xB6, 0x96, 0x0B, 0x3A, 0xAB, 0xE6,
            ],
        },

        local_rtp_options: None,
        remote_rtp_options: None,

        local_rtcp_options: None,
        remote_rtcp_options: None,
    };

    let sa = Session::new(Arc::new(ua), ca, false).await?;
    let sb = Session::new(Arc::new(ub), cb, false).await?;

    Ok((sa, sb))
}

const TEST_SSRC: u32 = 5000;

#[tokio::test]
async fn test_session_srtcp_accept() -> Result<()> {
    let (sa, sb) = build_session_srtcp_pair().await?;

    let rtcp_packet = picture_loss_indication::PictureLossIndication {
        media_ssrc: TEST_SSRC,
        ..Default::default()
    };

    let test_payload = rtcp_packet.marshal()?;
    sa.write_rtcp(&rtcp_packet).await?;

    let read_stream = sb.accept().await?;
    let ssrc = read_stream.get_ssrc();
    assert_eq!(
        ssrc, TEST_SSRC,
        "SSRC mismatch during accept exp({TEST_SSRC}) actual({ssrc})"
    );

    let mut read_buffer = BytesMut::with_capacity(test_payload.len());
    read_buffer.resize(test_payload.len(), 0u8);
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
async fn test_session_srtcp_listen() -> Result<()> {
    let (sa, sb) = build_session_srtcp_pair().await?;

    let rtcp_packet = picture_loss_indication::PictureLossIndication {
        media_ssrc: TEST_SSRC,
        ..Default::default()
    };

    let test_payload = rtcp_packet.marshal()?;
    let read_stream = sb.open(TEST_SSRC).await;

    sa.write_rtcp(&rtcp_packet).await?;

    let mut read_buffer = BytesMut::with_capacity(test_payload.len());
    read_buffer.resize(test_payload.len(), 0u8);
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

fn encrypt_srtcp(
    context: &mut Context,
    pkt: &(dyn rtcp::packet::Packet + Send + Sync),
) -> Result<Bytes> {
    let decrypted = pkt.marshal()?;
    let encrypted = context.encrypt_rtcp(&decrypted)?;
    Ok(encrypted)
}

const PLI_PACKET_SIZE: usize = 8;

async fn get_sender_ssrc(read_stream: &Arc<Stream>) -> Result<u32> {
    let auth_tag_size = ProtectionProfile::Aes128CmHmacSha1_80.rtcp_auth_tag_len();

    let mut read_buffer = BytesMut::with_capacity(PLI_PACKET_SIZE + auth_tag_size);
    read_buffer.resize(PLI_PACKET_SIZE + auth_tag_size, 0u8);

    let pkts = read_stream.read_rtcp(&mut read_buffer).await?;
    let mut bytes = &pkts[0].marshal()?[..];
    let pli = picture_loss_indication::PictureLossIndication::unmarshal(&mut bytes)?;

    Ok(pli.sender_ssrc)
}

#[tokio::test]
async fn test_session_srtcp_replay_protection() -> Result<()> {
    let (sa, sb) = build_session_srtcp_pair().await?;

    let read_stream = sb.open(TEST_SSRC).await;

    // Generate test packets
    let mut packets = vec![];
    let mut expected_ssrc = vec![];
    {
        let mut local_context = sa.local_context.lock().await;
        for i in 0..0x10u32 {
            expected_ssrc.push(i);

            let packet = picture_loss_indication::PictureLossIndication {
                media_ssrc: TEST_SSRC,
                sender_ssrc: i,
            };

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
            match get_sender_ssrc(&read_stream).await {
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
