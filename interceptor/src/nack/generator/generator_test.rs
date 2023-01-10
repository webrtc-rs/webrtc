use super::*;
use crate::mock::mock_stream::MockStream;
use crate::stream_info::RTCPFeedback;
use crate::test::timeout_or_fail;

use rtcp::transport_feedbacks::transport_layer_nack::TransportLayerNack;

#[tokio::test]
async fn test_generator_interceptor() -> Result<()> {
    const INTERVAL: Duration = Duration::from_millis(10);
    let icpr: Arc<dyn Interceptor + Send + Sync> = Generator::builder()
        .with_log2_size_minus_6(0)
        .with_skip_last_n(2)
        .with_interval(INTERVAL)
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

    for seq_num in [10, 11, 12, 14, 16, 18] {
        stream
            .receive_rtp(rtp::packet::Packet {
                header: rtp::header::Header {
                    sequence_number: seq_num,
                    ..Default::default()
                },
                ..Default::default()
            })
            .await;

        let r = timeout_or_fail(Duration::from_millis(10), stream.read_rtp())
            .await
            .expect("A read packet")
            .expect("Not an error");
        assert_eq!(r.header.sequence_number, seq_num);
    }

    tokio::time::sleep(INTERVAL * 2).await; // wait for at least 2 nack packets

    // ignore the first nack, it might only contain the sequence id 13 as missing
    let _ = stream.written_rtcp().await;

    let r = timeout_or_fail(Duration::from_millis(10), stream.written_rtcp())
        .await
        .expect("Write rtcp");
    if let Some(p) = r[0].as_any().downcast_ref::<TransportLayerNack>() {
        assert_eq!(p.nacks[0].packet_id, 13);
        assert_eq!(p.nacks[0].lost_packets, 0b10); // we want packets: 13, 15 (not packet 17, because skipLastN is setReceived to 2)
    } else {
        panic!("single packet RTCP Compound Packet expected");
    }

    stream.close().await?;

    Ok(())
}
