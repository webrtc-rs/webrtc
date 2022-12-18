use super::*;
use crate::mock::mock_stream::MockStream;
use crate::stream_info::RTCPFeedback;
use tokio::time::Duration;

use rtcp::transport_feedbacks::transport_layer_nack::{NackPair, TransportLayerNack};

#[tokio::test(start_paused = true)]
async fn test_responder_interceptor() -> Result<()> {
    let icpr: Arc<dyn Interceptor + Send + Sync> =
        Responder::builder().with_log2_size(3).build("")?;

    let stream = MockStream::new(
        &StreamInfo {
            ssrc: 1,
            rtcp_feedback: vec![RTCPFeedback {
                typ: "nack".to_owned(),
                ..Default::default()
            }],
            ..Default::default()
        },
        icpr,
    )
    .await;

    for seq_num in [10, 11, 12, 14, 15] {
        stream
            .write_rtp(&rtp::packet::Packet {
                header: rtp::header::Header {
                    sequence_number: seq_num,
                    ..Default::default()
                },
                ..Default::default()
            })
            .await?;

        // Let the packet be pulled through interceptor chains
        tokio::task::yield_now().await;

        let p = stream
            .written_rtp_expected()
            .await
            .expect("Packet should have been written");
        assert_eq!(seq_num, p.header.sequence_number);
    }

    stream
        .receive_rtcp(vec![Box::new(TransportLayerNack {
            media_ssrc: 1,
            sender_ssrc: 2,
            nacks: vec![
                NackPair {
                    packet_id: 11,
                    lost_packets: 0b1011,
                }, // sequence numbers: 11, 12, 13, 15
            ],
        })])
        .await;
    tokio::time::advance(Duration::from_millis(50)).await;
    // Let the NACK task do its thing
    tokio::task::yield_now().await;

    // seq number 13 was never sent, so it can't be resent
    for seq_num in [11, 12, 15] {
        let p = stream
            .written_rtp_expected()
            .await
            .expect("Packet should have been written");
        assert_eq!(seq_num, p.header.sequence_number);
    }

    let result = stream.written_rtp_expected().await;
    assert!(result.is_none(), "no more rtp packets expected");

    stream.close().await?;

    Ok(())
}

#[tokio::test(start_paused = true)]
async fn test_responder_interceptor_with_max_age() -> Result<()> {
    let icpr: Arc<dyn Interceptor + Send + Sync> = Responder::builder()
        .with_log2_size(3)
        .with_max_packet_age(Duration::from_millis(400))
        .build("")?;

    let stream = MockStream::new(
        &StreamInfo {
            ssrc: 1,
            rtcp_feedback: vec![RTCPFeedback {
                typ: "nack".to_owned(),
                ..Default::default()
            }],
            ..Default::default()
        },
        icpr,
    )
    .await;

    for seq_num in [10, 11, 12, 14, 15] {
        stream
            .write_rtp(&rtp::packet::Packet {
                header: rtp::header::Header {
                    sequence_number: seq_num,
                    ..Default::default()
                },
                ..Default::default()
            })
            .await?;
        tokio::time::advance(Duration::from_millis(30)).await;
        tokio::task::yield_now().await;

        let p = stream.written_rtp().await.expect("A packet");
        assert_eq!(seq_num, p.header.sequence_number);
    }

    // Advance time 300ms. Packets 10 and 11 will now have been sent 450ms and 420ms ago
    // respectively.
    tokio::time::advance(Duration::from_millis(300)).await;

    stream
        .receive_rtcp(vec![Box::new(TransportLayerNack {
            media_ssrc: 1,
            sender_ssrc: 2,
            nacks: vec![
                NackPair {
                    packet_id: 10,
                    lost_packets: 0b10111,
                }, // sequence numbers: 11, 12, 13, 15
            ],
        })])
        .await;
    tokio::task::yield_now().await;

    // seq number 13 was never sent and seq number 10 and 11 is too late to resend now.
    for seq_num in [12, 15] {
        if let Some(p) = stream.written_rtp().await {
            assert_eq!(seq_num, p.header.sequence_number);
        } else {
            assert!(
                false,
                "seq_num {} is not sent due to channel closed",
                seq_num
            );
        }
    }

    // Resume time
    tokio::time::resume();
    let result = tokio::time::timeout(Duration::from_millis(10), stream.written_rtp()).await;
    assert!(result.is_err(), "no more rtp packets expected");

    stream.close().await?;

    Ok(())
}
