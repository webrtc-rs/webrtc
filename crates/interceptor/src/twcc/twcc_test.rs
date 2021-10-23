use super::*;
use crate::error::Result;
use util::Marshal;

#[test]
fn test_chunk_add() -> Result<()> {
    //"fill with not received"
    {
        let mut c = Chunk::default();

        for i in 0..MAX_RUN_LENGTH_CAP {
            assert!(c.can_add(SymbolTypeTcc::PacketNotReceived as u16), "{}", i);
            c.add(SymbolTypeTcc::PacketNotReceived as u16);
        }
        assert_eq!(vec![0u16; MAX_RUN_LENGTH_CAP], c.deltas);
        assert!(!c.has_different_types);

        assert!(!c.can_add(SymbolTypeTcc::PacketNotReceived as u16));
        assert!(!c.can_add(SymbolTypeTcc::PacketReceivedSmallDelta as u16));
        assert!(!c.can_add(SymbolTypeTcc::PacketReceivedLargeDelta as u16));

        let status_chunk = c.encode();
        match status_chunk {
            PacketStatusChunk::RunLengthChunk(_) => assert!(true),
            _ => assert!(false),
        };

        let buf = status_chunk.marshal()?;
        assert_eq!(&[0x1f, 0xff], &buf[..]);
    }

    //"fill with small delta"
    {
        let mut c = Chunk::default();

        for i in 0..MAX_ONE_BIT_CAP {
            assert!(
                c.can_add(SymbolTypeTcc::PacketReceivedSmallDelta as u16),
                "{}",
                i
            );
            c.add(SymbolTypeTcc::PacketReceivedSmallDelta as u16);
        }

        assert_eq!(c.deltas, vec![1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1]);
        assert!(!c.has_different_types);

        assert!(!c.can_add(SymbolTypeTcc::PacketReceivedLargeDelta as u16));
        assert!(!c.can_add(SymbolTypeTcc::PacketNotReceived as u16));

        let status_chunk = c.encode();
        match status_chunk {
            PacketStatusChunk::RunLengthChunk(_) => assert!(true),
            _ => assert!(false),
        };

        let buf = status_chunk.marshal()?;
        assert_eq!(&[0x20, 0xe], &buf[..]);
    }

    //"fill with large delta"
    {
        let mut c = Chunk::default();

        for i in 0..MAX_TWO_BIT_CAP {
            assert!(
                c.can_add(SymbolTypeTcc::PacketReceivedLargeDelta as u16),
                "{}",
                i
            );
            c.add(SymbolTypeTcc::PacketReceivedLargeDelta as u16);
        }

        assert_eq!(c.deltas, vec![2, 2, 2, 2, 2, 2, 2]);
        assert!(c.has_large_delta);
        assert!(!c.has_different_types);

        assert!(!c.can_add(SymbolTypeTcc::PacketReceivedSmallDelta as u16));
        assert!(!c.can_add(SymbolTypeTcc::PacketNotReceived as u16));

        let status_chunk = c.encode();
        match status_chunk {
            PacketStatusChunk::RunLengthChunk(_) => assert!(true),
            _ => assert!(false),
        };

        let buf = status_chunk.marshal()?;
        assert_eq!(&[0x40, 0x7], &buf[..]);
    }

    // "fill with different types"
    {
        let mut c = Chunk::default();

        assert!(c.can_add(SymbolTypeTcc::PacketReceivedSmallDelta as u16));
        c.add(SymbolTypeTcc::PacketReceivedSmallDelta as u16);
        assert!(c.can_add(SymbolTypeTcc::PacketReceivedSmallDelta as u16));
        c.add(SymbolTypeTcc::PacketReceivedSmallDelta as u16);
        assert!(c.can_add(SymbolTypeTcc::PacketReceivedSmallDelta as u16));
        c.add(SymbolTypeTcc::PacketReceivedSmallDelta as u16);
        assert!(c.can_add(SymbolTypeTcc::PacketReceivedSmallDelta as u16));
        c.add(SymbolTypeTcc::PacketReceivedSmallDelta as u16);

        assert!(c.can_add(SymbolTypeTcc::PacketReceivedLargeDelta as u16));
        c.add(SymbolTypeTcc::PacketReceivedLargeDelta as u16);
        assert!(c.can_add(SymbolTypeTcc::PacketReceivedLargeDelta as u16));
        c.add(SymbolTypeTcc::PacketReceivedLargeDelta as u16);
        assert!(c.can_add(SymbolTypeTcc::PacketReceivedLargeDelta as u16));
        c.add(SymbolTypeTcc::PacketReceivedLargeDelta as u16);

        assert!(!c.can_add(SymbolTypeTcc::PacketReceivedLargeDelta as u16));

        let status_chunk = c.encode();
        match status_chunk {
            PacketStatusChunk::StatusVectorChunk(_) => assert!(true),
            _ => assert!(false),
        };

        let buf = status_chunk.marshal()?;
        assert_eq!(&[0xd5, 0x6a], &buf[..]);
    }

    //"overfill and encode"
    {
        let mut c = Chunk::default();

        assert!(c.can_add(SymbolTypeTcc::PacketReceivedSmallDelta as u16));
        c.add(SymbolTypeTcc::PacketReceivedSmallDelta as u16);
        assert!(c.can_add(SymbolTypeTcc::PacketNotReceived as u16));
        c.add(SymbolTypeTcc::PacketNotReceived as u16);
        assert!(c.can_add(SymbolTypeTcc::PacketNotReceived as u16));
        c.add(SymbolTypeTcc::PacketNotReceived as u16);
        assert!(c.can_add(SymbolTypeTcc::PacketNotReceived as u16));
        c.add(SymbolTypeTcc::PacketNotReceived as u16);
        assert!(c.can_add(SymbolTypeTcc::PacketNotReceived as u16));
        c.add(SymbolTypeTcc::PacketNotReceived as u16);
        assert!(c.can_add(SymbolTypeTcc::PacketNotReceived as u16));
        c.add(SymbolTypeTcc::PacketNotReceived as u16);
        assert!(c.can_add(SymbolTypeTcc::PacketNotReceived as u16));
        c.add(SymbolTypeTcc::PacketNotReceived as u16);
        assert!(c.can_add(SymbolTypeTcc::PacketNotReceived as u16));
        c.add(SymbolTypeTcc::PacketNotReceived as u16);

        assert!(!c.can_add(SymbolTypeTcc::PacketReceivedLargeDelta as u16));

        let status_chunk1 = c.encode();
        match status_chunk1 {
            PacketStatusChunk::StatusVectorChunk(_) => assert!(true),
            _ => assert!(false),
        };
        assert_eq!(1, c.deltas.len());

        assert!(c.can_add(SymbolTypeTcc::PacketReceivedLargeDelta as u16));
        c.add(SymbolTypeTcc::PacketReceivedLargeDelta as u16);

        let status_chunk2 = c.encode();
        match status_chunk2 {
            PacketStatusChunk::StatusVectorChunk(_) => assert!(true),
            _ => assert!(false),
        };
        assert_eq!(0, c.deltas.len());

        assert_eq!(
            PacketStatusChunk::StatusVectorChunk(StatusVectorChunk {
                type_tcc: StatusChunkTypeTcc::StatusVectorChunk,
                symbol_size: SymbolSizeTypeTcc::TwoBit,
                symbol_list: vec![
                    SymbolTypeTcc::PacketNotReceived,
                    SymbolTypeTcc::PacketReceivedLargeDelta
                ],
            }),
            status_chunk2
        );
    }

    Ok(())
}
/*
func Test_feedback(t *testing.T) {
    t.Run("add simple", func(t *testing.T) {
        f := feedback{}

        got := f.addReceived(0, 10)

        assert!( got)
    })

    t.Run("add too large", func(t *testing.T) {
        f := feedback{}

        assert.False(t, f.addReceived(12, 8200*1000*250))
    })

    t.Run("add received 1", func(t *testing.T) {
        f := &feedback{}
        f.setBase(1, 1000*1000)

        got := f.addReceived(1, 1023*1000)

        assert!( got)
        assert.Equal(t, uint16(2), f.nextSequenceNumber)
        assert.Equal(t, int64(15), f.refTimestamp64MS)

        got = f.addReceived(4, 1086*1000)
        assert!( got)
        assert.Equal(t, uint16(5), f.nextSequenceNumber)
        assert.Equal(t, int64(15), f.refTimestamp64MS)

        assert!( f.lastChunk.hasDifferentTypes)
        assert.Equal(t, 4, len(f.lastChunk.deltas))
        assert.NotContains(t, f.lastChunk.deltas, SymbolTypeTcc::PacketReceivedLargeDelta)
    })

    t.Run("add received 2", func(t *testing.T) {
        f := newFeedback(0, 0, 0)
        f.setBase(5, 320*1000)

        got := f.addReceived(5, 320*1000)
        assert!( got)
        got = f.addReceived(7, 448*1000)
        assert!( got)
        got = f.addReceived(8, 512*1000)
        assert!( got)
        got = f.addReceived(11, 768*1000)
        assert!( got)

        pkt := f.getRTCP()

        assert!( pkt.Header.Padding)
        assert.Equal(t, uint16(7), pkt.Header.Length)
        assert.Equal(t, uint16(5), pkt.BaseSequenceNumber)
        assert.Equal(t, uint16(7), pkt.PacketStatusCount)
        assert.Equal(t, uint32(5), pkt.ReferenceTime)
        assert.Equal(t, uint8(0), pkt.FbPktCount)
        assert.Equal(t, 1, len(pkt.PacketChunks))

        assert.Equal(t, []rtcp.PacketStatusChunk{&rtcp.StatusVectorChunk{
            SymbolSize: SymbolTypeTcc::SymbolSizeTwoBit,
            SymbolList: []uint16{
                SymbolTypeTcc::PacketReceivedSmallDelta,
                SymbolTypeTcc::PacketNotReceived,
                SymbolTypeTcc::PacketReceivedLargeDelta,
                SymbolTypeTcc::PacketReceivedLargeDelta,
                SymbolTypeTcc::PacketNotReceived,
                SymbolTypeTcc::PacketNotReceived,
                SymbolTypeTcc::PacketReceivedLargeDelta,
            },
        }}, pkt.PacketChunks)

        expectedDeltas := []*rtcp.RecvDelta{
            {
                Type:  SymbolTypeTcc::PacketReceivedSmallDelta,
                Delta: 0,
            },
            {
                Type:  SymbolTypeTcc::PacketReceivedLargeDelta,
                Delta: 0x0200 * SymbolTypeTcc::DeltaScaleFactor,
            },
            {
                Type:  SymbolTypeTcc::PacketReceivedLargeDelta,
                Delta: 0x0100 * SymbolTypeTcc::DeltaScaleFactor,
            },
            {
                Type:  SymbolTypeTcc::PacketReceivedLargeDelta,
                Delta: 0x0400 * SymbolTypeTcc::DeltaScaleFactor,
            },
        }
        assert.Equal(t, len(expectedDeltas), len(pkt.RecvDeltas))
        for i, d := range expectedDeltas {
            assert.Equal(t, d, pkt.RecvDeltas[i])
        }
    })

    t.Run("add received wrapped sequence number", func(t *testing.T) {
        f := newFeedback(0, 0, 0)
        f.setBase(65535, 320*1000)

        got := f.addReceived(65535, 320*1000)
        assert!( got)
        got = f.addReceived(7, 448*1000)
        assert!( got)
        got = f.addReceived(8, 512*1000)
        assert!( got)
        got = f.addReceived(11, 768*1000)
        assert!( got)

        pkt := f.getRTCP()

        assert!( pkt.Header.Padding)
        assert.Equal(t, uint16(7), pkt.Header.Length)
        assert.Equal(t, uint16(65535), pkt.BaseSequenceNumber)
        assert.Equal(t, uint16(13), pkt.PacketStatusCount)
        assert.Equal(t, uint32(5), pkt.ReferenceTime)
        assert.Equal(t, uint8(0), pkt.FbPktCount)
        assert.Equal(t, 2, len(pkt.PacketChunks))

        assert.Equal(t, []rtcp.PacketStatusChunk{
            &rtcp.StatusVectorChunk{
                SymbolSize: SymbolTypeTcc::SymbolSizeTwoBit,
                SymbolList: []uint16{
                    SymbolTypeTcc::PacketReceivedSmallDelta,
                    SymbolTypeTcc::PacketNotReceived,
                    SymbolTypeTcc::PacketNotReceived,
                    SymbolTypeTcc::PacketNotReceived,
                    SymbolTypeTcc::PacketNotReceived,
                    SymbolTypeTcc::PacketNotReceived,
                    SymbolTypeTcc::PacketNotReceived,
                },
            },
            &rtcp.StatusVectorChunk{
                SymbolSize: SymbolTypeTcc::SymbolSizeTwoBit,
                SymbolList: []uint16{
                    SymbolTypeTcc::PacketNotReceived,
                    SymbolTypeTcc::PacketReceivedLargeDelta,
                    SymbolTypeTcc::PacketReceivedLargeDelta,
                    SymbolTypeTcc::PacketNotReceived,
                    SymbolTypeTcc::PacketNotReceived,
                    SymbolTypeTcc::PacketReceivedLargeDelta,
                },
            },
        }, pkt.PacketChunks)

        expectedDeltas := []*rtcp.RecvDelta{
            {
                Type:  SymbolTypeTcc::PacketReceivedSmallDelta,
                Delta: 0,
            },
            {
                Type:  SymbolTypeTcc::PacketReceivedLargeDelta,
                Delta: 0x0200 * SymbolTypeTcc::DeltaScaleFactor,
            },
            {
                Type:  SymbolTypeTcc::PacketReceivedLargeDelta,
                Delta: 0x0100 * SymbolTypeTcc::DeltaScaleFactor,
            },
            {
                Type:  SymbolTypeTcc::PacketReceivedLargeDelta,
                Delta: 0x0400 * SymbolTypeTcc::DeltaScaleFactor,
            },
        }
        assert.Equal(t, len(expectedDeltas), len(pkt.RecvDeltas))
        for i, d := range expectedDeltas {
            assert.Equal(t, d, pkt.RecvDeltas[i])
        }
    })

    t.Run("get RTCP", func(t *testing.T) {
        testcases := []struct {
            arrivalTS              int64
            sequenceNumber         uint16
            wantRefTime            uint32
            wantBaseSequenceNumber uint16
        }{
            {320, 1, 5, 1},
            {1000, 2, 15, 2},
        }
        for _, tt := range testcases {
            tt := tt

            t.Run("set correct base seq and time", func(t *testing.T) {
                f := newFeedback(0, 0, 0)
                f.setBase(tt.sequenceNumber, tt.arrivalTS*1000)

                got := f.getRTCP()
                assert.Equal(t, tt.wantRefTime, got.ReferenceTime)
                assert.Equal(t, tt.wantBaseSequenceNumber, got.BaseSequenceNumber)
            })
        }
    })
}

func addRun(t *testing.T, r *Recorder, sequenceNumbers []uint16, arrivalTimes []int64) {
    assert.Equal(t, len(sequenceNumbers), len(arrivalTimes))

    for i := range sequenceNumbers {
        r.Record(5000, sequenceNumbers[i], arrivalTimes[i])
    }
}

const (
    scaleFactorReferenceTime = 64000
)

func increaseTime(arrivalTime *int64, increaseAmount int64) int64 {
    *arrivalTime += increaseAmount
    return *arrivalTime
}

func marshalAll(t *testing.T, pkts []rtcp.Packet) {
    for _, pkt := range pkts {
        _, err := pkt.Marshal()
        assert.NoError(t, err)
    }
}

func TestBuildFeedbackPacket(t *testing.T) {
    r := NewRecorder(5000)

    arrivalTime := int64(scaleFactorReferenceTime)
    addRun(t, r, []uint16{0, 1, 2, 3, 4, 5, 6, 7}, []int64{
        scaleFactorReferenceTime,
        increaseTime(&arrivalTime, SymbolTypeTcc::DeltaScaleFactor),
        increaseTime(&arrivalTime, SymbolTypeTcc::DeltaScaleFactor),
        increaseTime(&arrivalTime, SymbolTypeTcc::DeltaScaleFactor),
        increaseTime(&arrivalTime, SymbolTypeTcc::DeltaScaleFactor),
        increaseTime(&arrivalTime, SymbolTypeTcc::DeltaScaleFactor),
        increaseTime(&arrivalTime, SymbolTypeTcc::DeltaScaleFactor),
        increaseTime(&arrivalTime, SymbolTypeTcc::DeltaScaleFactor*256),
    })

    rtcpPackets := r.BuildFeedbackPacket()
    assert.Equal(t, 1, len(rtcpPackets))

    assert.Equal(t, &rtcp.TransportLayerCC{
        Header: rtcp.Header{
            Count:   rtcp.FormatTCC,
            Type:    rtcp.TypeTransportSpecificFeedback,
            Padding: true,
            Length:  8,
        },
        SenderSSRC:         5000,
        MediaSSRC:          5000,
        BaseSequenceNumber: 0,
        ReferenceTime:      1,
        FbPktCount:         0,
        PacketStatusCount:  8,
        PacketChunks: []rtcp.PacketStatusChunk{
            &rtcp.RunLengthChunk{
                Type:               SymbolTypeTcc::RunLengthChunk,
                PacketStatusSymbol: SymbolTypeTcc::PacketReceivedSmallDelta,
                RunLength:          7,
            },
            &rtcp.RunLengthChunk{
                Type:               SymbolTypeTcc::RunLengthChunk,
                PacketStatusSymbol: SymbolTypeTcc::PacketReceivedLargeDelta,
                RunLength:          1,
            },
        },
        RecvDeltas: []*rtcp.RecvDelta{
            {Type: SymbolTypeTcc::PacketReceivedSmallDelta, Delta: 0},
            {Type: SymbolTypeTcc::PacketReceivedSmallDelta, Delta: SymbolTypeTcc::DeltaScaleFactor},
            {Type: SymbolTypeTcc::PacketReceivedSmallDelta, Delta: SymbolTypeTcc::DeltaScaleFactor},
            {Type: SymbolTypeTcc::PacketReceivedSmallDelta, Delta: SymbolTypeTcc::DeltaScaleFactor},
            {Type: SymbolTypeTcc::PacketReceivedSmallDelta, Delta: SymbolTypeTcc::DeltaScaleFactor},
            {Type: SymbolTypeTcc::PacketReceivedSmallDelta, Delta: SymbolTypeTcc::DeltaScaleFactor},
            {Type: SymbolTypeTcc::PacketReceivedSmallDelta, Delta: SymbolTypeTcc::DeltaScaleFactor},
            {Type: SymbolTypeTcc::PacketReceivedLargeDelta, Delta: SymbolTypeTcc::DeltaScaleFactor * 256},
        },
    }, rtcpPackets[0].(*rtcp.TransportLayerCC))
    marshalAll(t, rtcpPackets)
}

func TestBuildFeedbackPacket_Rolling(t *testing.T) {
    r := NewRecorder(5000)

    arrivalTime := int64(scaleFactorReferenceTime)
    addRun(t, r, []uint16{0}, []int64{
        arrivalTime,
    })

    rtcpPackets := r.BuildFeedbackPacket()
    assert.Equal(t, 1, len(rtcpPackets)) // Empty TWCC

    addRun(t, r, []uint16{4, 5, 6, 7}, []int64{
        increaseTime(&arrivalTime, SymbolTypeTcc::DeltaScaleFactor),
        increaseTime(&arrivalTime, SymbolTypeTcc::DeltaScaleFactor),
        increaseTime(&arrivalTime, SymbolTypeTcc::DeltaScaleFactor),
        increaseTime(&arrivalTime, SymbolTypeTcc::DeltaScaleFactor),
    })

    rtcpPackets = r.BuildFeedbackPacket()
    assert.Equal(t, 1, len(rtcpPackets))

    assert.Equal(t, &rtcp.TransportLayerCC{
        Header: rtcp.Header{
            Count:   rtcp.FormatTCC,
            Type:    rtcp.TypeTransportSpecificFeedback,
            Padding: true,
            Length:  6,
        },
        SenderSSRC:         5000,
        MediaSSRC:          5000,
        BaseSequenceNumber: 0,
        ReferenceTime:      1,
        FbPktCount:         0,
        PacketStatusCount:  8,
        PacketChunks: []rtcp.PacketStatusChunk{
            &rtcp.StatusVectorChunk{
                Type:       SymbolTypeTcc::RunLengthChunk,
                SymbolSize: SymbolTypeTcc::SymbolSizeTwoBit,
                SymbolList: []uint16{
                    SymbolTypeTcc::PacketReceivedSmallDelta,
                    SymbolTypeTcc::PacketNotReceived,
                    SymbolTypeTcc::PacketNotReceived,
                    SymbolTypeTcc::PacketNotReceived,
                    SymbolTypeTcc::PacketReceivedSmallDelta,
                    SymbolTypeTcc::PacketReceivedSmallDelta,
                    SymbolTypeTcc::PacketReceivedSmallDelta,
                },
            },
        },
        RecvDeltas: []*rtcp.RecvDelta{
            {Type: SymbolTypeTcc::PacketReceivedSmallDelta, Delta: 0},
            {Type: SymbolTypeTcc::PacketReceivedSmallDelta, Delta: SymbolTypeTcc::DeltaScaleFactor},
            {Type: SymbolTypeTcc::PacketReceivedSmallDelta, Delta: SymbolTypeTcc::DeltaScaleFactor},
            {Type: SymbolTypeTcc::PacketReceivedSmallDelta, Delta: SymbolTypeTcc::DeltaScaleFactor},
            {Type: SymbolTypeTcc::PacketReceivedSmallDelta, Delta: SymbolTypeTcc::DeltaScaleFactor},
        },
    }, rtcpPackets[0].(*rtcp.TransportLayerCC))
    marshalAll(t, rtcpPackets)
}
*/
