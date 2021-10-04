use super::*;
use crate::mock::mock_stream::MockStream;
use crate::mock::mock_time::MockTime;
use chrono::prelude::*;
use rtp::extension::abs_send_time_extension::unix2ntp;
use std::future::Future;
use std::pin::Pin;

#[tokio::test]
async fn test_sender_interceptor_before_any_packet() -> Result<()> {
    let mt = Arc::new(MockTime::default());
    let mt2 = Arc::clone(&mt);
    let time_gen = Arc::new(
        move || -> Pin<Box<dyn Future<Output = SystemTime> + Send + 'static>> {
            let mt3 = Arc::clone(&mt2);
            Box::pin(async move { mt3.now().await })
        },
    );

    let icpr: Arc<dyn Interceptor + Send + Sync> = Arc::new(
        SenderReport::builder()
            .with_interval(Duration::from_millis(50))
            .with_now_fn(time_gen)
            .build_sr(),
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

    let dt = Utc.ymd(2009, 10, 23).and_hms(0, 0, 0);
    mt.set_now(dt.into()).await;

    let pkt = stream.written_rtcp().await.unwrap();

    if let Some(sr) = pkt
        .as_any()
        .downcast_ref::<rtcp::sender_report::SenderReport>()
    {
        assert_eq!(
            &rtcp::sender_report::SenderReport {
                ssrc: 123456,
                ntp_time: unix2ntp(mt.now().await),
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

/*
async fn test_sender_interceptor_after_rtp_packets() ->Result<()> {
    mt := &test.MockTime{}
    i, err := NewSenderInterceptor(
        SenderInterval(time.Millisecond*50),
        SenderLog(logging.NewDefaultLoggerFactory().NewLogger("test")),
        SenderNow(mt.Now),
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
        assert.NoError(t, stream.write_rtp(&rtp.Packet{
            Header:  rtp.Header{SequenceNumber: uint16(i)},
            Payload: []byte("\x00\x00"),
        }))
    }

    mt.SetNow(time.Date(2009, time.November, 10, 23, 0, 0, 0, time.UTC))
    pkts := <-stream.written_rtcp()
    assert.Equal(t, len(pkts), 1)
    sr, ok := pkts[0].(*rtcp.SenderReport)
    assert.True(t, ok)
    assert.Equal(t, &rtcp.SenderReport{
        SSRC:        123456,
        NTPTime:     ntpTime(mt.Now()),
        RTPTime:     2269117121,
        PacketCount: 10,
        OctetCount:  20,
    }, sr)

    Ok(())
}
*/
