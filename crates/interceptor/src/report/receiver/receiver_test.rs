use super::*;
use crate::mock::mock_stream::MockStream;
use crate::mock::mock_time::MockTime;
//use bytes::Bytes;
//use chrono::prelude::*;
//use rtp::extension::abs_send_time_extension::unix2ntp;
use std::future::Future;
use std::pin::Pin;

#[tokio::test]
async fn test_receiver_interceptor_before_any_packet() -> Result<()> {
    let mt = Arc::new(MockTime::default());
    let mt2 = Arc::clone(&mt);
    let time_gen = Arc::new(
        move || -> Pin<Box<dyn Future<Output = SystemTime> + Send + 'static>> {
            let mt3 = Arc::clone(&mt2);
            Box::pin(async move { mt3.now().await })
        },
    );

    let icpr: Arc<dyn Interceptor + Send + Sync> = Arc::new(
        ReceiverReport::builder()
            .with_interval(Duration::from_millis(50))
            .with_now_fn(time_gen)
            .build_rr(),
    );

    let stream = MockStream::new(
        &StreamInfo {
            ssrc: 123456,
            clock_rate: 90000,
            ..Default::default()
        },
        icpr,
    )
    .await;

    let pkt = stream.written_rtcp().await.unwrap();

    if let Some(rr) = pkt
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
    let mt2 = Arc::clone(&mt);
    let time_gen = Arc::new(
        move || -> Pin<Box<dyn Future<Output = SystemTime> + Send + 'static>> {
            let mt3 = Arc::clone(&mt2);
            Box::pin(async move { mt3.now().await })
        },
    );

    let icpr: Arc<dyn Interceptor + Send + Sync> = Arc::new(
        ReceiverReport::builder()
            .with_interval(Duration::from_millis(50))
            .with_now_fn(time_gen)
            .build_rr(),
    );

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

    let pkt = stream.written_rtcp().await.unwrap();

    if let Some(rr) = pkt
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
/*
#[tokio::test]
async fn  TestReceiverInterceptor_after_RTP_and_RTCP_packets() -> Result<()> {
        let rtpTime = Utc.ymd(2009, 10, 23).and_hms(0, 0, 0);

        mt := test.MockTime{}
        i, err := NewReceiverInterceptor(
            ReceiverInterval(time.Millisecond*50),
            ReceiverLog(logging.NewDefaultLoggerFactory().NewLogger("test")),
            ReceiverNow(mt.Now),
        )
        assert.NoError(t, err)

        stream := test.NewMockStream(&interceptor.StreamInfo{
            SSRC:      123456,
            ClockRate: 90000,
        }, i)
        defer func() {
            assert.NoError(t, stream.Close())
        }()

        for i := 0; i < 10; i++ {
            stream.ReceiveRTP(&rtp.Packet{Header: rtp.Header{
                SequenceNumber: uint16(i),
            }})
        }

        now := time.Date(2009, time.November, 10, 23, 0, 1, 0, time.UTC)
        stream.ReceiveRTCP([]rtcp.Packet{
            &rtcp.SenderReport{
                SSRC:        123456,
                NTPTime:     ntpTime(now),
                RTPTime:     987654321 + uint32(now.Sub(rtpTime).Seconds()*90000),
                PacketCount: 10,
                OctetCount:  0,
            },
        })

        pkts := <-stream.WrittenRTCP()
        assert.Equal(t, len(pkts), 1)
        rr, ok := pkts[0].(*rtcp.ReceiverReport)
        assert.True(t, ok)
        assert.Equal(t, 1, len(rr.Reports))
        assert.Equal(t, rtcp.ReceptionReport{
            SSRC:               uint32(123456),
            LastSequenceNumber: 9,
            LastSenderReport:   1861287936,
            FractionLost:       0,
            TotalLost:          0,
            Delay:              rr.Reports[0].Delay,
            Jitter:             0,
        }, rr.Reports[0])

stream.close().await?;

        Ok(())
    }

#[tokio::test]
async fn  TestReceiverInterceptor_overflow() -> Result<()> {
        mt := test.MockTime{}
        i, err := NewReceiverInterceptor(
            ReceiverInterval(time.Millisecond*50),
            ReceiverLog(logging.NewDefaultLoggerFactory().NewLogger("test")),
            ReceiverNow(mt.Now),
        )
        assert.NoError(t, err)

        stream := test.NewMockStream(&interceptor.StreamInfo{
            SSRC:      123456,
            ClockRate: 90000,
        }, i)
        defer func() {
            assert.NoError(t, stream.Close())
        }()

        stream.ReceiveRTP(&rtp.Packet{Header: rtp.Header{
            SequenceNumber: 0xffff,
        }})

        stream.ReceiveRTP(&rtp.Packet{Header: rtp.Header{
            SequenceNumber: 0x00,
        }})

        pkts := <-stream.WrittenRTCP()
        assert.Equal(t, len(pkts), 1)
        rr, ok := pkts[0].(*rtcp.ReceiverReport)
        assert.True(t, ok)
        assert.Equal(t, 1, len(rr.Reports))
        assert.Equal(t, rtcp.ReceptionReport{
            SSRC:               uint32(123456),
            LastSequenceNumber: 1<<16 | 0x0000,
            LastSenderReport:   0,
            FractionLost:       0,
            TotalLost:          0,
            Delay:              0,
            Jitter:             0,
        }, rr.Reports[0])

stream.close().await?;
        Ok(())
    }

#[tokio::test]
async fn  TestReceiverInterceptor_packet_loss() -> Result<()> {
    rtpTime := time.Date(2009, time.November, 10, 23, 0, 0, 0, time.UTC)

        mt := test.MockTime{}
        i, err := NewReceiverInterceptor(
            ReceiverInterval(time.Millisecond*50),
            ReceiverLog(logging.NewDefaultLoggerFactory().NewLogger("test")),
            ReceiverNow(mt.Now),
        )
        assert.NoError(t, err)

        stream := test.NewMockStream(&interceptor.StreamInfo{
            SSRC:      123456,
            ClockRate: 90000,
        }, i)
        defer func() {
            assert.NoError(t, stream.Close())
        }()

        stream.ReceiveRTP(&rtp.Packet{Header: rtp.Header{
            SequenceNumber: 0x01,
        }})

        stream.ReceiveRTP(&rtp.Packet{Header: rtp.Header{
            SequenceNumber: 0x03,
        }})

        pkts := <-stream.WrittenRTCP()
        assert.Equal(t, len(pkts), 1)
        rr, ok := pkts[0].(*rtcp.ReceiverReport)
        assert.True(t, ok)
        assert.Equal(t, 1, len(rr.Reports))
        assert.Equal(t, rtcp.ReceptionReport{
            SSRC:               uint32(123456),
            LastSequenceNumber: 0x03,
            LastSenderReport:   0,
            FractionLost:       256 * 1 / 3,
            TotalLost:          1,
            Delay:              0,
            Jitter:             0,
        }, rr.Reports[0])

        now := time.Date(2009, time.November, 10, 23, 0, 1, 0, time.UTC)
        stream.ReceiveRTCP([]rtcp.Packet{
            &rtcp.SenderReport{
                SSRC:        123456,
                NTPTime:     ntpTime(now),
                RTPTime:     987654321 + uint32(now.Sub(rtpTime).Seconds()*90000),
                PacketCount: 10,
                OctetCount:  0,
            },
        })

        pkts = <-stream.WrittenRTCP()
        assert.Equal(t, len(pkts), 1)
        rr, ok = pkts[0].(*rtcp.ReceiverReport)
        assert.True(t, ok)
        assert.Equal(t, 1, len(rr.Reports))
        assert.Equal(t, rtcp.ReceptionReport{
            SSRC:               uint32(123456),
            LastSequenceNumber: 0x03,
            LastSenderReport:   1861287936,
            FractionLost:       0,
            TotalLost:          1,
            Delay:              rr.Reports[0].Delay,
            Jitter:             0,
        }, rr.Reports[0])

stream.close().await?;
        Ok(())
    }

#[tokio::test]
async fn  TestReceiverInterceptor_overflow_and_packet_loss() -> Result<()> {
        mt := test.MockTime{}
        i, err := NewReceiverInterceptor(
            ReceiverInterval(time.Millisecond*50),
            ReceiverLog(logging.NewDefaultLoggerFactory().NewLogger("test")),
            ReceiverNow(mt.Now),
        )
        assert.NoError(t, err)

        stream := test.NewMockStream(&interceptor.StreamInfo{
            SSRC:      123456,
            ClockRate: 90000,
        }, i)
        defer func() {
            assert.NoError(t, stream.Close())
        }()

        stream.ReceiveRTP(&rtp.Packet{Header: rtp.Header{
            SequenceNumber: 0xffff,
        }})

        stream.ReceiveRTP(&rtp.Packet{Header: rtp.Header{
            SequenceNumber: 0x01,
        }})

        pkts := <-stream.WrittenRTCP()
        assert.Equal(t, len(pkts), 1)
        rr, ok := pkts[0].(*rtcp.ReceiverReport)
        assert.True(t, ok)
        assert.Equal(t, 1, len(rr.Reports))
        assert.Equal(t, rtcp.ReceptionReport{
            SSRC:               uint32(123456),
            LastSequenceNumber: 1<<16 | 0x01,
            LastSenderReport:   0,
            FractionLost:       256 * 1 / 3,
            TotalLost:          1,
            Delay:              0,
            Jitter:             0,
        }, rr.Reports[0])

stream.close().await?;
        Ok(())
    }

#[tokio::test]
async fn  TestReceiverInterceptor_reordered_packets() -> Result<()> {

        mt := test.MockTime{}
        i, err := NewReceiverInterceptor(
            ReceiverInterval(time.Millisecond*50),
            ReceiverLog(logging.NewDefaultLoggerFactory().NewLogger("test")),
            ReceiverNow(mt.Now),
        )
        assert.NoError(t, err)

        stream := test.NewMockStream(&interceptor.StreamInfo{
            SSRC:      123456,
            ClockRate: 90000,
        }, i)
        defer func() {
            assert.NoError(t, stream.Close())
        }()

        for _, seqNum := range []uint16{0x01, 0x03, 0x02, 0x04} {
            stream.ReceiveRTP(&rtp.Packet{Header: rtp.Header{
                SequenceNumber: seqNum,
            }})
        }

        pkts := <-stream.WrittenRTCP()
        assert.Equal(t, len(pkts), 1)
        rr, ok := pkts[0].(*rtcp.ReceiverReport)
        assert.True(t, ok)
        assert.Equal(t, 1, len(rr.Reports))
        assert.Equal(t, rtcp.ReceptionReport{
            SSRC:               uint32(123456),
            LastSequenceNumber: 0x04,
            LastSenderReport:   0,
            FractionLost:       0,
            TotalLost:          0,
            Delay:              0,
            Jitter:             0,
        }, rr.Reports[0])

stream.close().await?;
        Ok(())
    }

#[tokio::test]
async fn  TestReceiverInterceptorJitter() -> Result<()> {

        mt := test.MockTime{}
        i, err := NewReceiverInterceptor(
            ReceiverInterval(time.Millisecond*50),
            ReceiverLog(logging.NewDefaultLoggerFactory().NewLogger("test")),
            ReceiverNow(mt.Now),
        )
        assert.NoError(t, err)

        stream := test.NewMockStream(&interceptor.StreamInfo{
            SSRC:      123456,
            ClockRate: 90000,
        }, i)
        defer func() {
            assert.NoError(t, stream.Close())
        }()

        mt.SetNow(time.Date(2009, time.November, 10, 23, 0, 0, 0, time.UTC))
        stream.ReceiveRTP(&rtp.Packet{Header: rtp.Header{
            SequenceNumber: 0x01,
            Timestamp:      42378934,
        }})
        <-stream.ReadRTP()

        mt.SetNow(time.Date(2009, time.November, 10, 23, 0, 1, 0, time.UTC))
        stream.ReceiveRTP(&rtp.Packet{Header: rtp.Header{
            SequenceNumber: 0x02,
            Timestamp:      42378934 + 60000,
        }})

        pkts := <-stream.WrittenRTCP()
        assert.Equal(t, len(pkts), 1)
        rr, ok := pkts[0].(*rtcp.ReceiverReport)
        assert.True(t, ok)
        assert.Equal(t, 1, len(rr.Reports))
        assert.Equal(t, rtcp.ReceptionReport{
            SSRC:               uint32(123456),
            LastSequenceNumber: 0x02,
            LastSenderReport:   0,
            FractionLost:       0,
            TotalLost:          0,
            Delay:              0,
            Jitter:             30000 / 16,
        }, rr.Reports[0])

stream.close().await?;
        Ok(())
    }

#[tokio::test]
async fn  TestReceiverInterceptorDelay() -> Result<()> {
        mt := test.MockTime{}
        i, err := NewReceiverInterceptor(
            ReceiverInterval(time.Millisecond*50),
            ReceiverLog(logging.NewDefaultLoggerFactory().NewLogger("test")),
            ReceiverNow(mt.Now),
        )
        assert.NoError(t, err)

        stream := test.NewMockStream(&interceptor.StreamInfo{
            SSRC:      123456,
            ClockRate: 90000,
        }, i)
        defer func() {
            assert.NoError(t, stream.Close())
        }()

        mt.SetNow(time.Date(2009, time.November, 10, 23, 0, 0, 0, time.UTC))
        stream.ReceiveRTCP([]rtcp.Packet{
            &rtcp.SenderReport{
                SSRC:        123456,
                NTPTime:     ntpTime(time.Date(2009, time.November, 10, 23, 0, 0, 0, time.UTC)),
                RTPTime:     987654321,
                PacketCount: 0,
                OctetCount:  0,
            },
        })
        <-stream.ReadRTCP()

        mt.SetNow(time.Date(2009, time.November, 10, 23, 0, 1, 0, time.UTC))
        pkts := <-stream.WrittenRTCP()
        assert.Equal(t, len(pkts), 1)
        rr, ok := pkts[0].(*rtcp.ReceiverReport)
        assert.True(t, ok)
        assert.Equal(t, 1, len(rr.Reports))
        assert.Equal(t, rtcp.ReceptionReport{
            SSRC:               uint32(123456),
            LastSequenceNumber: 0,
            LastSenderReport:   1861222400,
            FractionLost:       0,
            TotalLost:          0,
            Delay:              65536,
            Jitter:             0,
        }, rr.Reports[0])

stream.close().await?;
    Ok(())
}*/
