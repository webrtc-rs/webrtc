use super::*;
use crate::mock::mock_time::SystemTimeMock;
use chrono::prelude::*;
use std::future::Future;
use std::pin::Pin;

#[tokio::test]
async fn test_sender_interceptor_before_any_packet() -> Result<()> {
    let mt = Arc::new(SystemTimeMock::default());
    let mt2 = Arc::clone(&mt);
    let time_gen = Arc::new(
        move || -> Pin<Box<dyn Future<Output = SystemTime> + Send + 'static>> {
            let mt3 = Arc::clone(&mt2);
            Box::pin(async move { mt3.now().await })
        },
    );

    let sr = SenderReport::builder()
        .with_interval(Duration::from_millis(50))
        .with_now_fn(time_gen)
        .build_sr();

    let dt = Utc.ymd(2009, 10, 23).and_hms(0, 0, 0);
    mt.set_now(dt.into()).await;
    /*
    stream := test.NewMockStream(&interceptor.StreamInfo{
        SSRC:      123456,
        ClockRate: 90000,
    }, i)
    defer func() {
        assert.NoError(t, stream.Close())
    }()


    pkts := <-stream.WrittenRTCP()
    assert.Equal(t, len(pkts), 1)
    sr, ok := pkts[0].(*rtcp.SenderReport)
    assert.True(t, ok)
    assert.Equal(t, &rtcp.SenderReport{
        SSRC:        123456,
        NTPTime:     ntpTime(mt.Now()),
        RTPTime:     2269117121,
        PacketCount: 0,
        OctetCount:  0,
    }, sr)*/

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
        assert.NoError(t, stream.WriteRTP(&rtp.Packet{
            Header:  rtp.Header{SequenceNumber: uint16(i)},
            Payload: []byte("\x00\x00"),
        }))
    }

    mt.SetNow(time.Date(2009, time.November, 10, 23, 0, 0, 0, time.UTC))
    pkts := <-stream.WrittenRTCP()
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
