use super::*;
use crate::error::Result;
use rtcp::packet::Packet;
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

#[test]
fn test_feedback() -> Result<()> {
    //"add simple"
    {
        let mut f = Feedback::default();
        let got = f.add_received(0, 10);
        assert!(got);
    }

    //"add too large"
    {
        let mut f = Feedback::default();

        assert!(!f.add_received(12, 8200 * 1000 * 250));
    }

    // "add received 1"
    {
        let mut f = Feedback::default();
        f.set_base(1, 1000 * 1000);

        let got = f.add_received(1, 1023 * 1000);

        assert!(got);
        assert_eq!(2, f.next_sequence_number);
        assert_eq!(15, f.ref_timestamp64ms);

        let got = f.add_received(4, 1086 * 1000);
        assert!(got);
        assert_eq!(5, f.next_sequence_number);
        assert_eq!(15, f.ref_timestamp64ms);

        assert!(f.last_chunk.has_different_types);
        assert_eq!(4, f.last_chunk.deltas.len());
        assert!(!f
            .last_chunk
            .deltas
            .contains(&(SymbolTypeTcc::PacketReceivedLargeDelta as u16)));
    }

    const TYPE_TCC_DELTA_SCALE_FACTOR: i64 = 250;

    //"add received 2"
    {
        let mut f = Feedback::new(0, 0, 0);
        f.set_base(5, 320 * 1000);

        let mut got = f.add_received(5, 320 * 1000);
        assert!(got);
        got = f.add_received(7, 448 * 1000);
        assert!(got);
        got = f.add_received(8, 512 * 1000);
        assert!(got);
        got = f.add_received(11, 768 * 1000);
        assert!(got);

        let pkt = f.get_rtcp();

        assert!(pkt.header().padding);
        assert_eq!(7, pkt.header().length);
        assert_eq!(5, pkt.base_sequence_number);
        assert_eq!(7, pkt.packet_status_count);
        assert_eq!(5, pkt.reference_time);
        assert_eq!(0, pkt.fb_pkt_count);
        assert_eq!(1, pkt.packet_chunks.len());

        assert_eq!(
            vec![PacketStatusChunk::StatusVectorChunk(StatusVectorChunk {
                type_tcc: StatusChunkTypeTcc::StatusVectorChunk,
                symbol_size: SymbolSizeTypeTcc::TwoBit,
                symbol_list: vec![
                    SymbolTypeTcc::PacketReceivedSmallDelta,
                    SymbolTypeTcc::PacketNotReceived,
                    SymbolTypeTcc::PacketReceivedLargeDelta,
                    SymbolTypeTcc::PacketReceivedLargeDelta,
                    SymbolTypeTcc::PacketNotReceived,
                    SymbolTypeTcc::PacketNotReceived,
                    SymbolTypeTcc::PacketReceivedLargeDelta,
                ],
            })],
            pkt.packet_chunks
        );

        let expected_deltas = vec![
            RecvDelta {
                type_tcc_packet: SymbolTypeTcc::PacketReceivedSmallDelta,
                delta: 0,
            },
            RecvDelta {
                type_tcc_packet: SymbolTypeTcc::PacketReceivedLargeDelta,
                delta: 0x0200 * TYPE_TCC_DELTA_SCALE_FACTOR,
            },
            RecvDelta {
                type_tcc_packet: SymbolTypeTcc::PacketReceivedLargeDelta,
                delta: 0x0100 * TYPE_TCC_DELTA_SCALE_FACTOR,
            },
            RecvDelta {
                type_tcc_packet: SymbolTypeTcc::PacketReceivedLargeDelta,
                delta: 0x0400 * TYPE_TCC_DELTA_SCALE_FACTOR,
            },
        ];
        assert_eq!(expected_deltas.len(), pkt.recv_deltas.len());
        for (i, d) in expected_deltas.iter().enumerate() {
            assert_eq!(d, &pkt.recv_deltas[i]);
        }
    }

    //"add received wrapped sequence number"
    {
        let mut f = Feedback::new(0, 0, 0);
        f.set_base(65535, 320 * 1000);

        let mut got = f.add_received(65535, 320 * 1000);
        assert!(got);
        got = f.add_received(7, 448 * 1000);
        assert!(got);
        got = f.add_received(8, 512 * 1000);
        assert!(got);
        got = f.add_received(11, 768 * 1000);
        assert!(got);

        let pkt = f.get_rtcp();

        assert!(pkt.header().padding);
        assert_eq!(7, pkt.header().length);
        assert_eq!(65535, pkt.base_sequence_number);
        assert_eq!(13, pkt.packet_status_count);
        assert_eq!(5, pkt.reference_time);
        assert_eq!(0, pkt.fb_pkt_count);
        assert_eq!(2, pkt.packet_chunks.len());

        assert_eq!(
            vec![
                PacketStatusChunk::StatusVectorChunk(StatusVectorChunk {
                    type_tcc: StatusChunkTypeTcc::StatusVectorChunk,
                    symbol_size: SymbolSizeTypeTcc::TwoBit,
                    symbol_list: vec![
                        SymbolTypeTcc::PacketReceivedSmallDelta,
                        SymbolTypeTcc::PacketNotReceived,
                        SymbolTypeTcc::PacketNotReceived,
                        SymbolTypeTcc::PacketNotReceived,
                        SymbolTypeTcc::PacketNotReceived,
                        SymbolTypeTcc::PacketNotReceived,
                        SymbolTypeTcc::PacketNotReceived,
                    ],
                }),
                PacketStatusChunk::StatusVectorChunk(StatusVectorChunk {
                    type_tcc: StatusChunkTypeTcc::StatusVectorChunk,
                    symbol_size: SymbolSizeTypeTcc::TwoBit,
                    symbol_list: vec![
                        SymbolTypeTcc::PacketNotReceived,
                        SymbolTypeTcc::PacketReceivedLargeDelta,
                        SymbolTypeTcc::PacketReceivedLargeDelta,
                        SymbolTypeTcc::PacketNotReceived,
                        SymbolTypeTcc::PacketNotReceived,
                        SymbolTypeTcc::PacketReceivedLargeDelta,
                    ],
                }),
            ],
            pkt.packet_chunks
        );

        let expected_deltas = vec![
            RecvDelta {
                type_tcc_packet: SymbolTypeTcc::PacketReceivedSmallDelta,
                delta: 0,
            },
            RecvDelta {
                type_tcc_packet: SymbolTypeTcc::PacketReceivedLargeDelta,
                delta: 0x0200 * TYPE_TCC_DELTA_SCALE_FACTOR,
            },
            RecvDelta {
                type_tcc_packet: SymbolTypeTcc::PacketReceivedLargeDelta,
                delta: 0x0100 * TYPE_TCC_DELTA_SCALE_FACTOR,
            },
            RecvDelta {
                type_tcc_packet: SymbolTypeTcc::PacketReceivedLargeDelta,
                delta: 0x0400 * TYPE_TCC_DELTA_SCALE_FACTOR,
            },
        ];
        assert_eq!(expected_deltas.len(), pkt.recv_deltas.len());
        for (i, d) in expected_deltas.iter().enumerate() {
            assert_eq!(d, &pkt.recv_deltas[i]);
        }
    }

    //"get RTCP"
    {
        let tests = vec![(320, 1, 5, 1), (1000, 2, 15, 2)];
        for (arrival_ts, sequence_number, want_ref_time, want_base_sequence_number) in tests {
            let mut f = Feedback::new(0, 0, 0);
            f.set_base(sequence_number, arrival_ts * 1000);

            let got = f.get_rtcp();
            assert_eq!(want_ref_time, got.reference_time);
            assert_eq!(want_base_sequence_number, got.base_sequence_number);
        }
    }

    Ok(())
}
/*
func addRun(t *testing.T, r *Recorder, sequenceNumbers []uint16, arrivalTimes []int64) {
    assert!( len(sequenceNumbers), len(arrivalTimes))

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
    assert!( 1, len(rtcpPackets))

    assert!( &rtcp.TransportLayerCC{
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
    assert!( 1, len(rtcpPackets)) // Empty TWCC

    addRun(t, r, []uint16{4, 5, 6, 7}, []int64{
        increaseTime(&arrivalTime, SymbolTypeTcc::DeltaScaleFactor),
        increaseTime(&arrivalTime, SymbolTypeTcc::DeltaScaleFactor),
        increaseTime(&arrivalTime, SymbolTypeTcc::DeltaScaleFactor),
        increaseTime(&arrivalTime, SymbolTypeTcc::DeltaScaleFactor),
    })

    rtcpPackets = r.BuildFeedbackPacket()
    assert!( 1, len(rtcpPackets))

    assert!( &rtcp.TransportLayerCC{
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
