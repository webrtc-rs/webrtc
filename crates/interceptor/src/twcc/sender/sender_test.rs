use super::*;
use crate::mock::mock_stream::MockStream;
use crate::stream_info::RTPHeaderExtension;
use rtcp::transport_feedbacks::transport_layer_cc::{
    PacketStatusChunk, RunLengthChunk, StatusChunkTypeTcc, StatusVectorChunk, SymbolSizeTypeTcc,
    SymbolTypeTcc, TransportLayerCc,
};
use util::Marshal;

#[tokio::test]
async fn test_sender_interceptor_before_any_packets() -> Result<()> {
    let builder = Sender::builder();
    let icpr = builder.build("")?;

    let stream = MockStream::new(
        &StreamInfo {
            ssrc: 1,
            rtp_header_extensions: vec![RTPHeaderExtension {
                uri: TRANSPORT_CC_URI.to_owned(),
                id: 1,
                ..Default::default()
            }],
            ..Default::default()
        },
        icpr,
    )
    .await;

    let pkt = stream.written_rtcp().await.unwrap();
    if let Some(tlcc) = pkt.as_any().downcast_ref::<TransportLayerCc>() {
        assert_eq!(0, tlcc.packet_status_count);
        assert_eq!(0, tlcc.fb_pkt_count);
        assert_eq!(0, tlcc.base_sequence_number);
        assert_eq!(0, tlcc.media_ssrc);
        assert_eq!(0, tlcc.reference_time);
        assert_eq!(0, tlcc.recv_deltas.len());
        assert_eq!(0, tlcc.packet_chunks.len());
    } else {
        assert!(false);
    }

    stream.close().await?;

    Ok(())
}

#[tokio::test]
async fn test_sender_interceptor_after_rtp_packets() -> Result<()> {
    let builder = Sender::builder();
    let icpr = builder.build("")?;

    let stream = MockStream::new(
        &StreamInfo {
            ssrc: 1,
            rtp_header_extensions: vec![RTPHeaderExtension {
                uri: TRANSPORT_CC_URI.to_owned(),
                id: 1,
                ..Default::default()
            }],
            ..Default::default()
        },
        icpr,
    )
    .await;

    for i in 0..10 {
        let mut hdr = rtp::header::Header::default();
        let tcc = TransportCcExtension {
            transport_sequence: i,
        }
        .marshal()?;
        hdr.set_extension(1, tcc)?;
        stream
            .receive_rtp(rtp::packet::Packet {
                header: hdr,
                ..Default::default()
            })
            .await;
    }

    let pkt = stream.written_rtcp().await.unwrap();
    if let Some(cc) = pkt.as_any().downcast_ref::<TransportLayerCc>() {
        assert_eq!(1, cc.media_ssrc);
        assert_eq!(0, cc.base_sequence_number);
        assert_eq!(
            vec![PacketStatusChunk::RunLengthChunk(RunLengthChunk {
                type_tcc: StatusChunkTypeTcc::RunLengthChunk,
                packet_status_symbol: SymbolTypeTcc::PacketReceivedSmallDelta,
                run_length: 10,
            })],
            cc.packet_chunks
        );
    } else {
        assert!(false);
    }

    stream.close().await?;

    Ok(())
}

#[tokio::test]
async fn test_sender_interceptor_different_delays_between_rtp_packets() -> Result<()> {
    let builder = Sender::builder().with_interval(Duration::from_millis(500));
    let icpr = builder.build("")?;

    let stream = MockStream::new(
        &StreamInfo {
            ssrc: 1,
            rtp_header_extensions: vec![RTPHeaderExtension {
                uri: TRANSPORT_CC_URI.to_owned(),
                id: 1,
                ..Default::default()
            }],
            ..Default::default()
        },
        icpr,
    )
    .await;

    let delays = vec![0, 10, 100, 200];
    for (i, d) in delays.iter().enumerate() {
        tokio::time::sleep(Duration::from_millis(*d)).await;

        let mut hdr = rtp::header::Header::default();
        let tcc = TransportCcExtension {
            transport_sequence: i as u16,
        }
        .marshal()?;

        hdr.set_extension(1, tcc)?;
        stream
            .receive_rtp(rtp::packet::Packet {
                header: hdr,
                ..Default::default()
            })
            .await;
    }

    // since tick immediately, let's ignore the first rtcp pkt
    let _ = stream.written_rtcp().await.unwrap();

    // the second 500ms tick will works
    let pkt = stream.written_rtcp().await.unwrap();
    if let Some(cc) = pkt.as_any().downcast_ref::<TransportLayerCc>() {
        assert_eq!(0, cc.base_sequence_number);
        assert_eq!(
            vec![PacketStatusChunk::StatusVectorChunk(StatusVectorChunk {
                type_tcc: StatusChunkTypeTcc::StatusVectorChunk,
                symbol_size: SymbolSizeTypeTcc::TwoBit,
                symbol_list: vec![
                    SymbolTypeTcc::PacketReceivedSmallDelta,
                    SymbolTypeTcc::PacketReceivedSmallDelta,
                    SymbolTypeTcc::PacketReceivedLargeDelta,
                    SymbolTypeTcc::PacketReceivedLargeDelta,
                ],
            })],
            cc.packet_chunks
        );
    } else {
        assert!(false);
    }

    stream.close().await?;

    Ok(())
}

/*
#[tokio::test]
async fn test_sender_interceptor_packet_loss() ->Result<()> {
    f, err := NewSenderInterceptor(SendInterval(2 * time.Second))
    assert.NoError(t, err)

    i, err := f.NewInterceptor("")
    assert.NoError(t, err)

    stream := test.NewMockStream(&interceptor.StreamInfo{RTPHeaderExtensions: []interceptor.RTPHeaderExtension{
        {
            URI: transportCCURI,
            ID:  1,
        },
    }}, i)
    defer func() {
        assert.NoError(t, stream.Close())
    }()

    sequenceNumberToDelay := map[int]int{
        0:  0,
        1:  10,
        4:  100,
        8:  200,
        9:  20,
        10: 20,
        30: 300,
    }
    for _, i := range []int{0, 1, 4, 8, 9, 10, 30} {
        d := sequenceNumberToDelay[i]
        time.Sleep(time.Duration(d) * time.Millisecond)

        hdr := rtp.Header{}
        tcc, err := (&rtp.TransportCCExtension{TransportSequence: uint16(i)}).Marshal()
        assert.NoError(t, err)
        err = hdr.SetExtension(1, tcc)
        assert.NoError(t, err)
        stream.ReceiveRTP(&rtp.Packet{Header: hdr})
    }

    pkts := <-stream.WrittenRTCP()
    assert.Equal(t, 1, len(pkts))
    cc, ok := pkts[0].(*rtcp.TransportLayerCC)
    assert.True(t, ok)
    assert.Equal(t, uint16(0), cc.BaseSequenceNumber)
    assert.Equal(t, []rtcp.PacketStatusChunk{
        &rtcp.StatusVectorChunk{
            SymbolSize: rtcp.TypeTCCSymbolSizeTwoBit,
            SymbolList: []uint16{
                rtcp.TypeTCCPacketReceivedSmallDelta,
                rtcp.TypeTCCPacketReceivedSmallDelta,
                rtcp.TypeTCCPacketNotReceived,
                rtcp.TypeTCCPacketNotReceived,
                rtcp.TypeTCCPacketReceivedLargeDelta,
                rtcp.TypeTCCPacketNotReceived,
                rtcp.TypeTCCPacketNotReceived,
            },
        },
        &rtcp.StatusVectorChunk{
            SymbolSize: rtcp.TypeTCCSymbolSizeTwoBit,
            SymbolList: []uint16{
                rtcp.TypeTCCPacketNotReceived,
                rtcp.TypeTCCPacketReceivedLargeDelta,
                rtcp.TypeTCCPacketReceivedSmallDelta,
                rtcp.TypeTCCPacketReceivedSmallDelta,
                rtcp.TypeTCCPacketNotReceived,
                rtcp.TypeTCCPacketNotReceived,
                rtcp.TypeTCCPacketNotReceived,
            },
        },
        &rtcp.RunLengthChunk{
            PacketStatusSymbol: rtcp.TypeTCCPacketNotReceived,
            RunLength:          16,
        },
        &rtcp.RunLengthChunk{
            PacketStatusSymbol: rtcp.TypeTCCPacketReceivedLargeDelta,
            RunLength:          1,
        },
    }, cc.PacketChunks)

    Ok(())
}

#[tokio::test]
async fn test_sender_interceptor_overflow() ->Result<()> {
    f, err := NewSenderInterceptor(SendInterval(2 * time.Second))
    assert.NoError(t, err)

    i, err := f.NewInterceptor("")
    assert.NoError(t, err)

    stream := test.NewMockStream(&interceptor.StreamInfo{RTPHeaderExtensions: []interceptor.RTPHeaderExtension{
        {
            URI: transportCCURI,
            ID:  1,
        },
    }}, i)
    defer func() {
        assert.NoError(t, stream.Close())
    }()

    for _, i := range []int{65530, 65534, 65535, 1, 2, 10} {
        hdr := rtp.Header{}
        tcc, err := (&rtp.TransportCCExtension{TransportSequence: uint16(i)}).Marshal()
        assert.NoError(t, err)
        err = hdr.SetExtension(1, tcc)
        assert.NoError(t, err)
        stream.ReceiveRTP(&rtp.Packet{Header: hdr})
    }

    pkts := <-stream.WrittenRTCP()
    assert.Equal(t, 1, len(pkts))
    cc, ok := pkts[0].(*rtcp.TransportLayerCC)
    assert.True(t, ok)
    assert.Equal(t, uint16(65530), cc.BaseSequenceNumber)
    assert.Equal(t, []rtcp.PacketStatusChunk{
        &rtcp.StatusVectorChunk{
            SymbolSize: rtcp.TypeTCCSymbolSizeOneBit,
            SymbolList: []uint16{
                rtcp.TypeTCCPacketReceivedSmallDelta,
                rtcp.TypeTCCPacketNotReceived,
                rtcp.TypeTCCPacketNotReceived,
                rtcp.TypeTCCPacketNotReceived,
                rtcp.TypeTCCPacketReceivedSmallDelta,
                rtcp.TypeTCCPacketReceivedSmallDelta,
                rtcp.TypeTCCPacketNotReceived,
                rtcp.TypeTCCPacketReceivedSmallDelta,
                rtcp.TypeTCCPacketReceivedSmallDelta,
                rtcp.TypeTCCPacketNotReceived,
                rtcp.TypeTCCPacketNotReceived,
                rtcp.TypeTCCPacketNotReceived,
                rtcp.TypeTCCPacketNotReceived,
                rtcp.TypeTCCPacketNotReceived,
            },
        },
        &rtcp.StatusVectorChunk{
            SymbolSize: rtcp.TypeTCCSymbolSizeTwoBit,
            SymbolList: []uint16{
                rtcp.TypeTCCPacketNotReceived,
                rtcp.TypeTCCPacketNotReceived,
                rtcp.TypeTCCPacketReceivedSmallDelta,
            },
        },
    }, cc.PacketChunks)
}
*/
