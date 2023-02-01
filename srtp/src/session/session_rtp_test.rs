use super::*;
use crate::error::Result;
use crate::protection_profile::*;

use bytes::{Bytes, BytesMut};
use std::{collections::HashMap, sync::Arc};
use tokio::{
    net::UdpSocket,
    sync::{mpsc, Mutex},
};

async fn build_session_srtp_pair() -> Result<(Session, Session)> {
    let ua = UdpSocket::bind("127.0.0.1:0").await?;
    let ub = UdpSocket::bind("127.0.0.1:0").await?;

    ua.connect(ub.local_addr()?).await?;
    ub.connect(ua.local_addr()?).await?;

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

    let sa = Session::new(Arc::new(ua), ca, true).await?;
    let sb = Session::new(Arc::new(ub), cb, true).await?;

    Ok((sa, sb))
}

const TEST_SSRC: u32 = 5000;
const RTP_HEADER_SIZE: usize = 12;

#[tokio::test]
async fn test_session_srtp_accept() -> Result<()> {
    let test_payload = Bytes::from_static(&[0x00, 0x01, 0x03, 0x04]);
    let mut read_buffer = BytesMut::with_capacity(RTP_HEADER_SIZE + test_payload.len());
    read_buffer.resize(RTP_HEADER_SIZE + test_payload.len(), 0u8);
    let (sa, sb) = build_session_srtp_pair().await?;

    let packet = rtp::packet::Packet {
        header: rtp::header::Header {
            ssrc: TEST_SSRC,
            ..Default::default()
        },
        payload: test_payload.clone(),
    };
    sa.write_rtp(&packet).await?;

    let read_stream = sb.accept().await?;
    let ssrc = read_stream.get_ssrc();
    assert_eq!(
        ssrc, TEST_SSRC,
        "SSRC mismatch during accept exp({TEST_SSRC}) actual({ssrc})"
    );

    read_stream.read(&mut read_buffer).await?;

    assert_eq!(
        &test_payload[..],
        &read_buffer[RTP_HEADER_SIZE..],
        "Sent buffer does not match the one received exp({:?}) actual({:?})",
        &test_payload[..],
        &read_buffer[RTP_HEADER_SIZE..]
    );

    sa.close().await?;
    sb.close().await?;

    Ok(())
}

#[tokio::test]
async fn test_session_srtp_listen() -> Result<()> {
    let test_payload = Bytes::from_static(&[0x00, 0x01, 0x03, 0x04]);
    let mut read_buffer = BytesMut::with_capacity(RTP_HEADER_SIZE + test_payload.len());
    read_buffer.resize(RTP_HEADER_SIZE + test_payload.len(), 0u8);
    let (sa, sb) = build_session_srtp_pair().await?;

    let packet = rtp::packet::Packet {
        header: rtp::header::Header {
            ssrc: TEST_SSRC,
            ..Default::default()
        },
        payload: test_payload.clone(),
    };

    let read_stream = sb.open(TEST_SSRC).await;

    sa.write_rtp(&packet).await?;

    read_stream.read(&mut read_buffer).await?;

    assert_eq!(
        &test_payload[..],
        &read_buffer[RTP_HEADER_SIZE..],
        "Sent buffer does not match the one received exp({:?}) actual({:?})",
        &test_payload[..],
        &read_buffer[RTP_HEADER_SIZE..]
    );

    sa.close().await?;
    sb.close().await?;

    Ok(())
}

#[tokio::test]
async fn test_session_srtp_multi_ssrc() -> Result<()> {
    let ssrcs = vec![5000, 5001, 5002];
    let test_payload = Bytes::from_static(&[0x00, 0x01, 0x03, 0x04]);
    let mut read_buffer = BytesMut::with_capacity(RTP_HEADER_SIZE + test_payload.len());
    read_buffer.resize(RTP_HEADER_SIZE + test_payload.len(), 0u8);
    let (sa, sb) = build_session_srtp_pair().await?;

    let mut read_streams = HashMap::new();
    for ssrc in &ssrcs {
        let read_stream = sb.open(*ssrc).await;
        read_streams.insert(*ssrc, read_stream);
    }

    for ssrc in &ssrcs {
        let packet = rtp::packet::Packet {
            header: rtp::header::Header {
                ssrc: *ssrc,
                ..Default::default()
            },
            payload: test_payload.clone(),
        };
        sa.write_rtp(&packet).await?;

        if let Some(read_stream) = read_streams.get_mut(ssrc) {
            read_stream.read(&mut read_buffer).await?;

            assert_eq!(
                &test_payload[..],
                &read_buffer[RTP_HEADER_SIZE..],
                "Sent buffer does not match the one received exp({:?}) actual({:?})",
                &test_payload[..],
                &read_buffer[RTP_HEADER_SIZE..]
            );
        } else {
            panic!("ssrc {} not found", *ssrc);
        }
    }

    sa.close().await?;
    sb.close().await?;

    Ok(())
}

fn encrypt_srtp(context: &mut Context, pkt: &rtp::packet::Packet) -> Result<Bytes> {
    let decrypted = pkt.marshal()?;
    let encrypted = context.encrypt_rtp(&decrypted)?;
    Ok(encrypted)
}

async fn payload_srtp(
    read_stream: &Arc<Stream>,
    header_size: usize,
    expected_payload: &[u8],
) -> Result<u16> {
    let mut read_buffer = BytesMut::with_capacity(header_size + expected_payload.len());
    read_buffer.resize(header_size + expected_payload.len(), 0u8);

    let (n, hdr) = read_stream.read_rtp(&mut read_buffer).await?;

    assert_eq!(
        expected_payload,
        &read_buffer[header_size..n],
        "Sent buffer does not match the one received exp({:?}) actual({:?})",
        expected_payload,
        &read_buffer[header_size..n]
    );

    Ok(hdr.sequence_number)
}

#[tokio::test]
async fn test_session_srtp_replay_protection() -> Result<()> {
    let test_payload = Bytes::from_static(&[0x00, 0x01, 0x03, 0x04]);

    let (sa, sb) = build_session_srtp_pair().await?;

    let read_stream = sb.open(TEST_SSRC).await;

    // Generate test packets
    let mut packets = vec![];
    let mut expected_sequence_number = vec![];
    {
        let mut local_context = sa.local_context.lock().await;
        let mut i = 0xFFF0u16;
        while i != 0x10 {
            expected_sequence_number.push(i);

            let packet = rtp::packet::Packet {
                header: rtp::header::Header {
                    ssrc: TEST_SSRC,
                    sequence_number: i,
                    ..Default::default()
                },
                payload: test_payload.clone(),
            };

            let encrypted = encrypt_srtp(&mut local_context, &packet)?;

            packets.push(encrypted);

            if i == 0xFFFF {
                i = 0;
            } else {
                i += 1;
            }
        }
    }

    let (done_tx, mut done_rx) = mpsc::channel::<()>(1);

    let received_sequence_number = Arc::new(Mutex::new(vec![]));
    let cloned_received_sequence_number = Arc::clone(&received_sequence_number);
    let count = expected_sequence_number.len();

    tokio::spawn(async move {
        let mut i = 0;
        while i < count {
            let seq = payload_srtp(&read_stream, RTP_HEADER_SIZE, &test_payload)
                .await
                .unwrap();
            let mut r = cloned_received_sequence_number.lock().await;
            r.push(seq);

            i += 1;
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
        let received_sequence_number = received_sequence_number.lock().await;
        assert_eq!(&received_sequence_number[..], &expected_sequence_number[..]);
    }

    Ok(())
}
