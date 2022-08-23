/*TODO:
use super::*;
use crate::api::APIBuilder;
use crate::peer_connection::configuration::RTCConfiguration;

use bytes::Bytes;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use interceptor::mock::mock_builder::MockBuilder;
use interceptor::mock::mock_interceptor::MockInterceptor;
use interceptor::stream_info::StreamInfo;
use interceptor::{Attributes, Interceptor, RTPWriter, RTPWriterFn};

// E2E test of the features of Interceptors
// * Assert an extension can be set on an outbound packet
// * Assert an extension can be read on an outbound packet
// * Assert that attributes set by an interceptor are returned to the Reader
#[tokio::test]
async fn test_peer_connection_interceptor() -> Result<()> {
    let create_pc = || async {
        let mut m = MediaEngine::default();
        m.register_default_codecs()?;

        let mut ir = Registry::new();

        let BindLocalStreamFn = |info: &StreamInfo,
                                 writer: Arc<dyn RTPWriter + Send + Sync>|
         -> Pin<
            Box<dyn Future<Output = Arc<dyn RTPWriter + Send + Sync>> + Send + Sync>,
        > {
            let writer2 = Arc::clone(&writer);
            Box::pin(async move {
                Arc::new(RTPWriterFn(Box::new(
                    move |in_pkt: &rtp::packet::Packet,
                          attributes: &Attributes|
                          -> Pin<
                        Box<
                            dyn Future<Output = std::result::Result<usize, interceptor::Error>>
                                + Send
                                + Sync,
                        >,
                    > {
                        let writer3 = Arc::clone(&writer2);
                        let a = attributes.clone();
                        // set extension on outgoing packet
                        let mut out_pkt = in_pkt.clone();
                        out_pkt.header.extension = true;
                        out_pkt.header.extension_profile = 0xBEDE;

                        Box::pin(async move {
                            out_pkt
                                .header
                                .set_extension(2, Bytes::from_static(b"foo"))?;
                            //writer3.write(&out_pkt, &a).await
                            Ok(0)
                        })
                    },
                ))) as Arc<dyn RTPWriter + Send + Sync>
            })
        };

        BindRemoteStreamFn: func(_ *interceptor.StreamInfo, reader interceptor.RTPReader) interceptor.RTPReader {
            return interceptor.RTPReaderFunc(func(b []byte, a interceptor.Attributes) (int, interceptor.Attributes, error) {
                if a == nil {
                    a = interceptor.Attributes{}
                }

                a.Set("attribute", "value")
                return reader.Read(b, a)
            })
        },
        let mock_builder = Box::new(MockBuilder {
            build:
                Box::new(
                    |_: &str| -> std::result::Result<
                        Arc<dyn Interceptor + Send + Sync>,
                        interceptor::Error,
                    > {
                        Ok(Arc::new(MockInterceptor {
                            ..Default::default()
                        }))
                    },
                ),
        });
        let mock_builder = MockBuilder::new(
                    |_: &str| -> std::result::Result<
                        Arc<dyn Interceptor + Send + Sync>,
                        interceptor::Error,
                    > {
                        Ok(Arc::new(MockInterceptor {
                            ..Default::default()
                        }))
                    },
                );
        ir.add(Box::new(mock_builder));

        let api = APIBuilder::new()
            .with_media_engine(m)
            .with_interceptor_registry(ir)
            .build();
        api.new_peer_connection(RTCConfiguration::default()).await
    };

    let offerer = create_pc().await?;
    let answerer = create_pc().await?;

    track, err := NewTrackLocalStaticSample(RTPCodecCapability{MimeType: MimeTypeVP8}, "video", "pion")
    assert.NoError(t, err)

    _, err = offerer.AddTrack(track)
    assert.NoError(t, err)

    seenRTP, seenRTPCancel := context.WithCancel(context.Background())
    answerer.OnTrack(func(track *TrackRemote, receiver *RTPReceiver) {
        p, attributes, readErr := track.ReadRTP()
        assert.NoError(t, readErr)

        assert.Equal(t, p.Extension, true)
        assert.Equal(t, "foo", string(p.GetExtension(2)))
        assert.Equal(t, "value", attributes.Get("attribute"))

        seenRTPCancel()
    })

    assert.NoError(t, signalPair(offerer, answerer))

    func() {
        ticker := time.NewTicker(time.Millisecond * 20)
        for {
            select {
            case <-seenRTP.Done():
                return
            case <-ticker.C:
                assert.NoError(t, track.WriteSample(media.Sample{Data: []byte{0x00}, Duration: time.Second}))
            }
        }
    }()

    closePairNow(t, offerer, answerer)

    Ok(())
}

func Test_Interceptor_BindUnbind(t *testing.T) {
    lim := test.TimeOut(time.Second * 10)
    defer lim.Stop()

    report := test.CheckRoutines(t)
    defer report()

    m := &MediaEngine{}
    assert.NoError(t, m.RegisterDefaultCodecs())

    var (
        cntBindRTCPReader     uint32
        cntBindRTCPWriter     uint32
        cntBindLocalStream    uint32
        cntUnbindLocalStream  uint32
        cntBindRemoteStream   uint32
        cntUnbindRemoteStream uint32
        cntClose              uint32
    )
    mockInterceptor := &mock_interceptor.Interceptor{
        BindRTCPReaderFn: func(reader interceptor.RTCPReader) interceptor.RTCPReader {
            atomic.AddUint32(&cntBindRTCPReader, 1)
            return reader
        },
        BindRTCPWriterFn: func(writer interceptor.RTCPWriter) interceptor.RTCPWriter {
            atomic.AddUint32(&cntBindRTCPWriter, 1)
            return writer
        },
        BindLocalStreamFn: func(i *interceptor.StreamInfo, writer interceptor.RTPWriter) interceptor.RTPWriter {
            atomic.AddUint32(&cntBindLocalStream, 1)
            return writer
        },
        UnbindLocalStreamFn: func(i *interceptor.StreamInfo) {
            atomic.AddUint32(&cntUnbindLocalStream, 1)
        },
        BindRemoteStreamFn: func(i *interceptor.StreamInfo, reader interceptor.RTPReader) interceptor.RTPReader {
            atomic.AddUint32(&cntBindRemoteStream, 1)
            return reader
        },
        UnbindRemoteStreamFn: func(i *interceptor.StreamInfo) {
            atomic.AddUint32(&cntUnbindRemoteStream, 1)
        },
        CloseFn: func() error {
            atomic.AddUint32(&cntClose, 1)
            return nil
        },
    }
    ir := &interceptor.Registry{}
    ir.Add(&mock_interceptor.Factory{
        NewInterceptorFn: func(_ string) (interceptor.Interceptor, error) { return mockInterceptor, nil },
    })

    sender, receiver, err := NewAPI(WithMediaEngine(m), WithInterceptorRegistry(ir)).newPair(Configuration{})
    assert.NoError(t, err)

    track, err := NewTrackLocalStaticSample(RTPCodecCapability{MimeType: MimeTypeVP8}, "video", "pion")
    assert.NoError(t, err)

    _, err = sender.AddTrack(track)
    assert.NoError(t, err)

    receiverReady, receiverReadyFn := context.WithCancel(context.Background())
    receiver.OnTrack(func(track *TrackRemote, _ *RTPReceiver) {
        _, _, readErr := track.ReadRTP()
        assert.NoError(t, readErr)
        receiverReadyFn()
    })

    assert.NoError(t, signalPair(sender, receiver))

    ticker := time.NewTicker(time.Millisecond * 20)
    defer ticker.Stop()
    func() {
        for {
            select {
            case <-receiverReady.Done():
                return
            case <-ticker.C:
                // Send packet to make receiver track actual creates RTPReceiver.
                assert.NoError(t, track.WriteSample(media.Sample{Data: []byte{0xAA}, Duration: time.Second}))
            }
        }
    }()

    closePairNow(t, sender, receiver)

    // Bind/UnbindLocal/RemoteStream should be called from one side.
    if cnt := atomic.LoadUint32(&cntBindLocalStream); cnt != 1 {
        t.Errorf("BindLocalStreamFn is expected to be called once, but called %d times", cnt)
    }
    if cnt := atomic.LoadUint32(&cntUnbindLocalStream); cnt != 1 {
        t.Errorf("UnbindLocalStreamFn is expected to be called once, but called %d times", cnt)
    }
    if cnt := atomic.LoadUint32(&cntBindRemoteStream); cnt != 1 {
        t.Errorf("BindRemoteStreamFn is expected to be called once, but called %d times", cnt)
    }
    if cnt := atomic.LoadUint32(&cntUnbindRemoteStream); cnt != 1 {
        t.Errorf("UnbindRemoteStreamFn is expected to be called once, but called %d times", cnt)
    }

    // BindRTCPWriter/Reader and Close should be called from both side.
    if cnt := atomic.LoadUint32(&cntBindRTCPWriter); cnt != 2 {
        t.Errorf("BindRTCPWriterFn is expected to be called twice, but called %d times", cnt)
    }
    if cnt := atomic.LoadUint32(&cntBindRTCPReader); cnt != 2 {
        t.Errorf("BindRTCPReaderFn is expected to be called twice, but called %d times", cnt)
    }
    if cnt := atomic.LoadUint32(&cntClose); cnt != 2 {
        t.Errorf("CloseFn is expected to be called twice, but called %d times", cnt)
    }
}

func Test_InterceptorRegistry_Build(t *testing.T) {
    registryBuildCount := 0

    ir := &interceptor.Registry{}
    ir.Add(&mock_interceptor.Factory{
        NewInterceptorFn: func(_ string) (interceptor.Interceptor, error) {
            registryBuildCount++
            return &interceptor.NoOp{}, nil
        },
    })

    peerConnectionA, err := NewAPI(WithInterceptorRegistry(ir)).NewPeerConnection(Configuration{})
    assert.NoError(t, err)

    peerConnectionB, err := NewAPI(WithInterceptorRegistry(ir)).NewPeerConnection(Configuration{})
    assert.NoError(t, err)

    assert.Equal(t, 2, registryBuildCount)
    closePairNow(t, peerConnectionA, peerConnectionB)
}
*/
