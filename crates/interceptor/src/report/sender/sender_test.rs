use super::*;
use crate::mock::mock_stream::MockStream;
use crate::mock::mock_time::MockTime;
use bytes::Bytes;
use chrono::prelude::*;
use rtp::extension::abs_send_time_extension::unix2ntp;
use std::future::Future;
use std::pin::Pin;

#[tokio::test]
async fn test_sender_interceptor_before_any_packet() -> Result<()> {
    let mt = Arc::new(MockTime::default());
    let time_gen = {
        let mt = Arc::clone(&mt);
        Arc::new(move || mt.now())
    };

    let icpr: Arc<dyn Interceptor + Send + Sync> = SenderReport::builder()
        .with_interval(Duration::from_millis(50))
        .with_now_fn(time_gen)
        .build("")?;

    let stream = MockStream::new(
        &StreamInfo {
            ssrc: 123456,
            clock_rate: 90000,
            ..Default::default()
        },
        icpr,
    )
    .await;

    let dt = Utc.ymd(2009, 10, 23).and_hms(0, 0, 0);
    mt.set_now(dt.into());

    let pkts = stream.written_rtcp().await.unwrap();
    assert_eq!(pkts.len(), 1);
    if let Some(sr) = pkts[0]
        .as_any()
        .downcast_ref::<rtcp::sender_report::SenderReport>()
    {
        assert_eq!(
            &rtcp::sender_report::SenderReport {
                ssrc: 123456,
                ntp_time: unix2ntp(mt.now()),
                rtp_time: 4294967295, // pion: 2269117121,
                packet_count: 0,
                octet_count: 0,
                ..Default::default()
            },
            sr
        )
    } else {
        assert!(false);
    }

    stream.close().await?;

    Ok(())
}

#[tokio::test]
async fn test_sender_interceptor_after_rtp_packets() -> Result<()> {
    let mt = Arc::new(MockTime::default());
    let time_gen = {
        let mt = Arc::clone(&mt);
        Arc::new(move || mt.now())
    };

    let icpr: Arc<dyn Interceptor + Send + Sync> = SenderReport::builder()
        .with_interval(Duration::from_millis(50))
        .with_now_fn(time_gen)
        .build("")?;

    let stream = MockStream::new(
        &StreamInfo {
            ssrc: 123456,
            clock_rate: 90000,
            ..Default::default()
        },
        icpr,
    )
    .await;

    for i in 0..10u16 {
        stream
            .write_rtp(&rtp::packet::Packet {
                header: rtp::header::Header {
                    sequence_number: i,
                    ..Default::default()
                },
                payload: Bytes::from_static(b"\x00\x00"),
            })
            .await?;
    }

    let dt = Utc.ymd(2009, 10, 23).and_hms(0, 0, 0);
    mt.set_now(dt.into());

    let pkts = stream.written_rtcp().await.unwrap();
    assert_eq!(pkts.len(), 1);
    if let Some(sr) = pkts[0]
        .as_any()
        .downcast_ref::<rtcp::sender_report::SenderReport>()
    {
        assert_eq!(
            &rtcp::sender_report::SenderReport {
                ssrc: 123456,
                ntp_time: unix2ntp(mt.now()),
                rtp_time: 4294967295, // pion: 2269117121,
                packet_count: 10,
                octet_count: 20,
                ..Default::default()
            },
            sr
        )
    } else {
        assert!(false);
    }

    stream.close().await?;

    Ok(())
}

#[tokio::test]
async fn test_sender_interceptor_after_rtp_packets_overflow() -> Result<()> {
    let mt = Arc::new(MockTime::default());
    let time_gen = {
        let mt = Arc::clone(&mt);
        Arc::new(move || mt.now())
    };

    let icpr: Arc<dyn Interceptor + Send + Sync> = SenderReport::builder()
        .with_interval(Duration::from_millis(50))
        .with_now_fn(time_gen)
        .build("")?;

    let stream = MockStream::new(
        &StreamInfo {
            ssrc: 123456,
            clock_rate: 90000,
            ..Default::default()
        },
        icpr,
    )
    .await;

    stream
        .write_rtp(&rtp::packet::Packet {
            header: rtp::header::Header {
                sequence_number: 0xfffd,
                ..Default::default()
            },
            payload: Bytes::from_static(b"\x00\x00"),
        })
        .await?;

    stream
        .write_rtp(&rtp::packet::Packet {
            header: rtp::header::Header {
                sequence_number: 0xfffe,
                ..Default::default()
            },
            payload: Bytes::from_static(b"\x00\x00"),
        })
        .await?;

    stream
        .write_rtp(&rtp::packet::Packet {
            header: rtp::header::Header {
                sequence_number: 0xffff,
                ..Default::default()
            },
            payload: Bytes::from_static(b"\x00\x00"),
        })
        .await?;

    stream
        .write_rtp(&rtp::packet::Packet {
            header: rtp::header::Header {
                sequence_number: 0,
                ..Default::default()
            },
            payload: Bytes::from_static(b"\x00\x00"),
        })
        .await?;

    stream
        .write_rtp(&rtp::packet::Packet {
            header: rtp::header::Header {
                sequence_number: 1,
                ..Default::default()
            },
            payload: Bytes::from_static(b"\x00\x00"),
        })
        .await?;

    let dt = Utc.ymd(2009, 10, 23).and_hms(0, 0, 0);
    mt.set_now(dt.into());

    let pkts = stream.written_rtcp().await.unwrap();
    assert_eq!(pkts.len(), 1);
    if let Some(sr) = pkts[0]
        .as_any()
        .downcast_ref::<rtcp::sender_report::SenderReport>()
    {
        assert_eq!(
            &rtcp::sender_report::SenderReport {
                ssrc: 123456,
                ntp_time: unix2ntp(mt.now()),
                rtp_time: 4294967295, // pion: 2269117121,
                packet_count: 5,
                octet_count: 10,
                ..Default::default()
            },
            sr
        )
    } else {
        assert!(false);
    }

    stream.close().await?;

    Ok(())
}

#[tokio::test]
async fn test_stream_counters_initially_zero() -> Result<()> {
    let counters = sender_stream::Counters::default();
    assert_eq!(0, counters.octet_count());
    assert_eq!(0, counters.packet_count());
    Ok(())
}

#[tokio::test]
async fn test_stream_packet_counter_wraps_on_overflow() -> Result<()> {
    let mut counters = sender_stream::Counters::mock(u32::MAX, 0);
    for _ in 0..3 {
        counters.increment_packets();
    }
    assert_eq!(2, counters.packet_count());
    Ok(())
}

#[tokio::test]
async fn test_stream_octet_counter_wraps_on_overflow() -> Result<()> {
    let mut counters = sender_stream::Counters::default();
    counters.count_octets(u32::MAX as usize);
    counters.count_octets(3);
    assert_eq!(2, counters.octet_count());
    Ok(())
}

#[tokio::test]
async fn test_stream_octet_counter_saturates_u32_from_usize() -> Result<()> {
    let mut counters = sender_stream::Counters::default();
    counters.count_octets(0xabcdef01234567_usize);
    assert_eq!(0xffffffff_u32, counters.octet_count());
    Ok(())
}
