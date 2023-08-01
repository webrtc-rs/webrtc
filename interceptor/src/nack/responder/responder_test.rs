use rtcp::transport_feedbacks::transport_layer_nack::{NackPair, TransportLayerNack};
use tokio::time::Duration;

use super::*;
use crate::mock::mock_stream::MockStream;
use crate::stream_info::RTCPFeedback;
use crate::test::timeout_or_fail;

#[tokio::test]
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

        let p = timeout_or_fail(Duration::from_millis(10), stream.written_rtp())
            .await
            .expect("A packet");
        assert_eq!(p.header.sequence_number, seq_num);
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

    // seq number 13 was never sent, so it can't be resent
    for seq_num in [11, 12, 15] {
        if let Ok(r) = tokio::time::timeout(Duration::from_millis(50), stream.written_rtp()).await {
            if let Some(p) = r {
                assert_eq!(p.header.sequence_number, seq_num);
            } else {
                panic!("seq_num {seq_num} is not sent due to channel closed");
            }
        } else {
            panic!("seq_num {seq_num} is not sent yet");
        }
    }

    let result = tokio::time::timeout(Duration::from_millis(10), stream.written_rtp()).await;
    assert!(result.is_err(), "no more rtp packets expected");

    stream.close().await?;

    Ok(())
}
