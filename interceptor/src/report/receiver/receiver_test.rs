use super::*;
use crate::mock::mock_stream::MockStream;
use crate::mock::mock_time::MockTime;
//use bytes::Bytes;
use chrono::prelude::*;
use rtp::extension::abs_send_time_extension::unix2ntp;

#[tokio::test]
async fn test_receiver_interceptor_before_any_packet() -> Result<()> {
    let mt = Arc::new(MockTime::default());
    let time_gen = {
        let mt = Arc::clone(&mt);
        Arc::new(move || mt.now())
    };

    let icpr: Arc<dyn Interceptor + Send + Sync> = ReceiverReport::builder()
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

    let pkts = stream.written_rtcp().await.unwrap();
    assert_eq!(pkts.len(), 1);

    if let Some(rr) = pkts[0]
        .as_any()
        .downcast_ref::<rtcp::receiver_report::ReceiverReport>()
    {
        assert_eq!(1, rr.reports.len());
        assert_eq!(
            rtcp::reception_report::ReceptionReport {
                ssrc: 123456,
                last_sequence_number: 0,
                last_sender_report: 0,
                fraction_lost: 0,
                total_lost: 0,
                delay: 0,
                jitter: 0,
            },
            rr.reports[0]
        )
    } else {
        assert!(false);
    }

    stream.close().await?;

    Ok(())
}

#[tokio::test]
async fn test_receiver_interceptor_after_rtp_packets() -> Result<()> {
    let mt = Arc::new(MockTime::default());
    let time_gen = {
        let mt = Arc::clone(&mt);
        Arc::new(move || mt.now())
    };

    let icpr: Arc<dyn Interceptor + Send + Sync> = ReceiverReport::builder()
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
            .receive_rtp(rtp::packet::Packet {
                header: rtp::header::Header {
                    sequence_number: i,
                    ..Default::default()
                },
                ..Default::default()
            })
            .await;
    }

    let pkts = stream.written_rtcp().await.unwrap();
    assert_eq!(pkts.len(), 1);
    if let Some(rr) = pkts[0]
        .as_any()
        .downcast_ref::<rtcp::receiver_report::ReceiverReport>()
    {
        assert_eq!(1, rr.reports.len());
        assert_eq!(
            rtcp::reception_report::ReceptionReport {
                ssrc: 123456,
                last_sequence_number: 9,
                last_sender_report: 0,
                fraction_lost: 0,
                total_lost: 0,
                delay: 0,
                jitter: 0,
            },
            rr.reports[0]
        )
    } else {
        assert!(false);
    }

    stream.close().await?;

    Ok(())
}

#[tokio::test]
async fn test_receiver_interceptor_after_rtp_and_rtcp_packets() -> Result<()> {
    let rtp_time: SystemTime = Utc.ymd(2009, 10, 23).and_hms(0, 0, 0).into();

    let mt = Arc::new(MockTime::default());
    let time_gen = {
        let mt = Arc::clone(&mt);
        Arc::new(move || mt.now())
    };

    let icpr: Arc<dyn Interceptor + Send + Sync> = ReceiverReport::builder()
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
            .receive_rtp(rtp::packet::Packet {
                header: rtp::header::Header {
                    sequence_number: i,
                    ..Default::default()
                },
                ..Default::default()
            })
            .await;
    }

    let now: SystemTime = Utc.ymd(2009, 11, 10).and_hms(23, 0, 1).into();
    let rt = 987654321u32.wrapping_add(
        (now.duration_since(rtp_time)
            .unwrap_or(Duration::from_secs(0))
            .as_secs_f64()
            * 90000.0) as u32,
    );
    stream
        .receive_rtcp(vec![Box::new(rtcp::sender_report::SenderReport {
            ssrc: 123456,
            ntp_time: unix2ntp(now),
            rtp_time: rt,
            packet_count: 10,
            octet_count: 0,
            ..Default::default()
        })])
        .await;

    let pkts = stream.written_rtcp().await.unwrap();
    assert_eq!(pkts.len(), 1);
    if let Some(rr) = pkts[0]
        .as_any()
        .downcast_ref::<rtcp::receiver_report::ReceiverReport>()
    {
        assert_eq!(1, rr.reports.len());
        assert_eq!(
            rtcp::reception_report::ReceptionReport {
                ssrc: 123456,
                last_sequence_number: 9,
                last_sender_report: 1861287936,
                fraction_lost: 0,
                total_lost: 0,
                delay: rr.reports[0].delay,
                jitter: 0,
            },
            rr.reports[0]
        )
    } else {
        assert!(false);
    }

    stream.close().await?;

    Ok(())
}

#[tokio::test]
async fn test_receiver_interceptor_overflow() -> Result<()> {
    let mt = Arc::new(MockTime::default());
    let _mt2 = Arc::clone(&mt);
    let time_gen = {
        let mt = Arc::clone(&mt);
        Arc::new(move || mt.now())
    };

    let icpr: Arc<dyn Interceptor + Send + Sync> = ReceiverReport::builder()
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
        .receive_rtp(rtp::packet::Packet {
            header: rtp::header::Header {
                sequence_number: 0xffff,
                ..Default::default()
            },
            ..Default::default()
        })
        .await;

    stream
        .receive_rtp(rtp::packet::Packet {
            header: rtp::header::Header {
                sequence_number: 0,
                ..Default::default()
            },
            ..Default::default()
        })
        .await;

    let pkts = stream.written_rtcp().await.unwrap();
    assert_eq!(pkts.len(), 1);
    if let Some(rr) = pkts[0]
        .as_any()
        .downcast_ref::<rtcp::receiver_report::ReceiverReport>()
    {
        assert_eq!(1, rr.reports.len());
        assert_eq!(
            rtcp::reception_report::ReceptionReport {
                ssrc: 123456,
                last_sequence_number: (1 << 16) | 0x0000,
                last_sender_report: 0,
                fraction_lost: 0,
                total_lost: 0,
                delay: rr.reports[0].delay,
                jitter: 0,
            },
            rr.reports[0]
        )
    } else {
        assert!(false);
    }

    stream.close().await?;
    Ok(())
}

#[tokio::test]
async fn test_receiver_interceptor_overflow_five_pkts() -> Result<()> {
    let mt = Arc::new(MockTime::default());
    let time_gen = {
        let mt = Arc::clone(&mt);
        Arc::new(move || mt.now())
    };

    let icpr: Arc<dyn Interceptor + Send + Sync> = ReceiverReport::builder()
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
        .receive_rtp(rtp::packet::Packet {
            header: rtp::header::Header {
                sequence_number: 0xfffd,
                ..Default::default()
            },
            ..Default::default()
        })
        .await;

    stream
        .receive_rtp(rtp::packet::Packet {
            header: rtp::header::Header {
                sequence_number: 0xfffe,
                ..Default::default()
            },
            ..Default::default()
        })
        .await;

    stream
        .receive_rtp(rtp::packet::Packet {
            header: rtp::header::Header {
                sequence_number: 0xffff,
                ..Default::default()
            },
            ..Default::default()
        })
        .await;

    stream
        .receive_rtp(rtp::packet::Packet {
            header: rtp::header::Header {
                sequence_number: 0,
                ..Default::default()
            },
            ..Default::default()
        })
        .await;

    stream
        .receive_rtp(rtp::packet::Packet {
            header: rtp::header::Header {
                sequence_number: 1,
                ..Default::default()
            },
            ..Default::default()
        })
        .await;

    let pkts = stream.written_rtcp().await.unwrap();
    assert_eq!(pkts.len(), 1);
    if let Some(rr) = pkts[0]
        .as_any()
        .downcast_ref::<rtcp::receiver_report::ReceiverReport>()
    {
        assert_eq!(1, rr.reports.len());
        assert_eq!(
            rtcp::reception_report::ReceptionReport {
                ssrc: 123456,
                last_sequence_number: (1 << 16) | 0x0001,
                last_sender_report: 0,
                fraction_lost: 0,
                total_lost: 0,
                delay: rr.reports[0].delay,
                jitter: 0,
            },
            rr.reports[0]
        )
    } else {
        assert!(false);
    }

    stream.close().await?;
    Ok(())
}

#[tokio::test]
async fn test_receiver_interceptor_packet_loss() -> Result<()> {
    let rtp_time: SystemTime = Utc.ymd(2009, 11, 10).and_hms(23, 0, 0).into();

    let mt = Arc::new(MockTime::default());
    let time_gen = {
        let mt = Arc::clone(&mt);
        Arc::new(move || mt.now())
    };

    let icpr: Arc<dyn Interceptor + Send + Sync> = ReceiverReport::builder()
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
        .receive_rtp(rtp::packet::Packet {
            header: rtp::header::Header {
                sequence_number: 0x01,
                ..Default::default()
            },
            ..Default::default()
        })
        .await;

    stream
        .receive_rtp(rtp::packet::Packet {
            header: rtp::header::Header {
                sequence_number: 0x03,
                ..Default::default()
            },
            ..Default::default()
        })
        .await;

    let pkts = stream.written_rtcp().await.unwrap();
    assert_eq!(pkts.len(), 1);
    if let Some(rr) = pkts[0]
        .as_any()
        .downcast_ref::<rtcp::receiver_report::ReceiverReport>()
    {
        assert_eq!(1, rr.reports.len());
        assert_eq!(
            rtcp::reception_report::ReceptionReport {
                ssrc: 123456,
                last_sequence_number: 0x03,
                last_sender_report: 0,
                fraction_lost: (256u16 * 1 / 3) as u8,
                total_lost: 1,
                delay: 0,
                jitter: 0,
            },
            rr.reports[0]
        )
    } else {
        assert!(false);
    }

    let now: SystemTime = Utc.ymd(2009, 11, 10).and_hms(23, 0, 1).into();
    let rt = 987654321u32.wrapping_add(
        (now.duration_since(rtp_time)
            .unwrap_or(Duration::from_secs(0))
            .as_secs_f64()
            * 90000.0) as u32,
    );
    stream
        .receive_rtcp(vec![Box::new(rtcp::sender_report::SenderReport {
            ssrc: 123456,
            ntp_time: unix2ntp(now),
            rtp_time: rt,
            packet_count: 10,
            octet_count: 0,
            ..Default::default()
        })])
        .await;

    let pkts = stream.written_rtcp().await.unwrap();
    assert_eq!(pkts.len(), 1);
    if let Some(rr) = pkts[0]
        .as_any()
        .downcast_ref::<rtcp::receiver_report::ReceiverReport>()
    {
        assert_eq!(1, rr.reports.len());
        assert_eq!(
            rtcp::reception_report::ReceptionReport {
                ssrc: 123456,
                last_sequence_number: 0x03,
                last_sender_report: 1861287936,
                fraction_lost: 0,
                total_lost: 1,
                delay: rr.reports[0].delay,
                jitter: 0,
            },
            rr.reports[0]
        )
    } else {
        assert!(false);
    }

    stream.close().await?;
    Ok(())
}

#[tokio::test]
async fn test_receiver_interceptor_overflow_and_packet_loss() -> Result<()> {
    let mt = Arc::new(MockTime::default());
    let time_gen = {
        let mt = Arc::clone(&mt);
        Arc::new(move || mt.now())
    };

    let icpr: Arc<dyn Interceptor + Send + Sync> = ReceiverReport::builder()
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
        .receive_rtp(rtp::packet::Packet {
            header: rtp::header::Header {
                sequence_number: 0xffff,
                ..Default::default()
            },
            ..Default::default()
        })
        .await;

    stream
        .receive_rtp(rtp::packet::Packet {
            header: rtp::header::Header {
                sequence_number: 0x01,
                ..Default::default()
            },
            ..Default::default()
        })
        .await;

    let pkts = stream.written_rtcp().await.unwrap();
    assert_eq!(pkts.len(), 1);
    if let Some(rr) = pkts[0]
        .as_any()
        .downcast_ref::<rtcp::receiver_report::ReceiverReport>()
    {
        assert_eq!(1, rr.reports.len());
        assert_eq!(
            rtcp::reception_report::ReceptionReport {
                ssrc: 123456,
                last_sequence_number: 1 << 16 | 0x01,
                last_sender_report: 0,
                fraction_lost: (256u16 * 1 / 3) as u8,
                total_lost: 1,
                delay: 0,
                jitter: 0,
            },
            rr.reports[0]
        )
    } else {
        assert!(false);
    }

    stream.close().await?;
    Ok(())
}

#[tokio::test]
async fn test_receiver_interceptor_reordered_packets() -> Result<()> {
    let mt = Arc::new(MockTime::default());
    let time_gen = {
        let mt = Arc::clone(&mt);
        Arc::new(move || mt.now())
    };

    let icpr: Arc<dyn Interceptor + Send + Sync> = ReceiverReport::builder()
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

    for sequence_number in [0x01, 0x03, 0x02, 0x04] {
        stream
            .receive_rtp(rtp::packet::Packet {
                header: rtp::header::Header {
                    sequence_number,
                    ..Default::default()
                },
                ..Default::default()
            })
            .await;
    }

    let pkts = stream.written_rtcp().await.unwrap();
    assert_eq!(pkts.len(), 1);
    if let Some(rr) = pkts[0]
        .as_any()
        .downcast_ref::<rtcp::receiver_report::ReceiverReport>()
    {
        assert_eq!(1, rr.reports.len());
        assert_eq!(
            rtcp::reception_report::ReceptionReport {
                ssrc: 123456,
                last_sequence_number: 0x04,
                last_sender_report: 0,
                fraction_lost: 0,
                total_lost: 0,
                delay: 0,
                jitter: 0,
            },
            rr.reports[0]
        )
    } else {
        assert!(false);
    }

    stream.close().await?;
    Ok(())
}

#[tokio::test]
async fn test_receiver_interceptor_jitter() -> Result<()> {
    let mt = Arc::new(MockTime::default());
    let time_gen = {
        let mt = Arc::clone(&mt);
        Arc::new(move || mt.now())
    };

    let icpr: Arc<dyn Interceptor + Send + Sync> = ReceiverReport::builder()
        .with_interval(Duration::from_millis(25))
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

    mt.set_now(Utc.ymd(2009, 11, 10).and_hms(23, 0, 0).into());
    stream
        .receive_rtp(rtp::packet::Packet {
            header: rtp::header::Header {
                sequence_number: 0x01,
                timestamp: 42378934,
                ..Default::default()
            },
            ..Default::default()
        })
        .await;
    stream.read_rtp().await;

    mt.set_now(Utc.ymd(2009, 11, 10).and_hms(23, 0, 1).into());
    stream
        .receive_rtp(rtp::packet::Packet {
            header: rtp::header::Header {
                sequence_number: 0x02,
                timestamp: 42378934 + 60000,
                ..Default::default()
            },
            ..Default::default()
        })
        .await;

    // Wait at least 50 ms to ensure a report is generated
    tokio::time::sleep(Duration::from_millis(50)).await;

    let pkts = stream.last_written_rtcp().await.unwrap();
    assert_eq!(pkts.len(), 1);
    if let Some(rr) = pkts[0]
        .as_any()
        .downcast_ref::<rtcp::receiver_report::ReceiverReport>()
    {
        assert_eq!(1, rr.reports.len());
        assert_eq!(
            rtcp::reception_report::ReceptionReport {
                ssrc: 123456,
                last_sequence_number: 0x02,
                last_sender_report: 0,
                fraction_lost: 0,
                total_lost: 0,
                delay: 0,
                jitter: 30000 / 16,
            },
            rr.reports[0]
        )
    } else {
        assert!(false);
    }

    stream.close().await?;
    Ok(())
}

#[tokio::test]
async fn test_receiver_interceptor_delay() -> Result<()> {
    let mt = Arc::new(MockTime::default());
    let time_gen = {
        let mt = Arc::clone(&mt);
        Arc::new(move || mt.now())
    };

    let icpr: Arc<dyn Interceptor + Send + Sync> = ReceiverReport::builder()
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

    mt.set_now(Utc.ymd(2009, 11, 10).and_hms(23, 0, 0).into());
    stream
        .receive_rtcp(vec![Box::new(rtcp::sender_report::SenderReport {
            ssrc: 123456,
            ntp_time: unix2ntp(Utc.ymd(2009, 11, 10).and_hms(23, 0, 0).into()),
            rtp_time: 987654321,
            packet_count: 0,
            octet_count: 0,
            ..Default::default()
        })])
        .await;
    stream.read_rtcp().await;

    mt.set_now(Utc.ymd(2009, 11, 10).and_hms(23, 0, 1).into());

    let pkts = stream.written_rtcp().await.unwrap();
    assert_eq!(pkts.len(), 1);
    if let Some(rr) = pkts[0]
        .as_any()
        .downcast_ref::<rtcp::receiver_report::ReceiverReport>()
    {
        assert_eq!(1, rr.reports.len());
        assert_eq!(
            rtcp::reception_report::ReceptionReport {
                ssrc: 123456,
                last_sequence_number: 0,
                last_sender_report: 1861222400,
                fraction_lost: 0,
                total_lost: 0,
                delay: 65536,
                jitter: 0,
            },
            rr.reports[0]
        )
    } else {
        assert!(false);
    }

    stream.close().await?;
    Ok(())
}
