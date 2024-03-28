use rtcp::packet::Packet;
use util::Marshal;

use super::*;
use crate::error::Result;

#[test]
fn test_chunk_add() -> Result<()> {
    //"fill with not received"
    {
        let mut c = Chunk::default();

        for i in 0..MAX_RUN_LENGTH_CAP {
            assert!(c.can_add(SymbolTypeTcc::PacketNotReceived as u16), "{}", i);
            c.add(SymbolTypeTcc::PacketNotReceived as u16);
        }
        assert_eq!(c.deltas, vec![0u16; MAX_RUN_LENGTH_CAP]);
        assert!(!c.has_different_types);

        assert!(!c.can_add(SymbolTypeTcc::PacketNotReceived as u16));
        assert!(!c.can_add(SymbolTypeTcc::PacketReceivedSmallDelta as u16));
        assert!(!c.can_add(SymbolTypeTcc::PacketReceivedLargeDelta as u16));

        let status_chunk = c.encode();
        match status_chunk {
            PacketStatusChunk::RunLengthChunk(_) => {}
            _ => panic!(),
        };

        let buf = status_chunk.marshal()?;
        assert_eq!(&buf[..], &[0x1f, 0xff]);
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
            PacketStatusChunk::RunLengthChunk(_) => {}
            _ => panic!(),
        };

        let buf = status_chunk.marshal()?;
        assert_eq!(&buf[..], &[0x20, 0xe]);
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
            PacketStatusChunk::RunLengthChunk(_) => {}
            _ => panic!(),
        };

        let buf = status_chunk.marshal()?;
        assert_eq!(&buf[..], &[0x40, 0x7]);
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
            PacketStatusChunk::StatusVectorChunk(_) => {}
            _ => panic!(),
        };

        let buf = status_chunk.marshal()?;
        assert_eq!(&buf[..], &[0xd5, 0x6a]);
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
            PacketStatusChunk::StatusVectorChunk(_) => {}
            _ => panic!(),
        };
        assert_eq!(c.deltas.len(), 1);

        assert!(c.can_add(SymbolTypeTcc::PacketReceivedLargeDelta as u16));
        c.add(SymbolTypeTcc::PacketReceivedLargeDelta as u16);

        let status_chunk2 = c.encode();
        match status_chunk2 {
            PacketStatusChunk::StatusVectorChunk(_) => {}
            _ => panic!(),
        };
        assert_eq!(c.deltas.len(), 0);

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
        assert_eq!(f.next_sequence_number, 2);
        assert_eq!(f.ref_timestamp64ms, 15);

        let got = f.add_received(4, 1086 * 1000);
        assert!(got);
        assert_eq!(f.next_sequence_number, 5);
        assert_eq!(f.ref_timestamp64ms, 15);

        assert!(f.last_chunk.has_different_types);
        assert_eq!(f.last_chunk.deltas.len(), 4);
        assert!(!f
            .last_chunk
            .deltas
            .contains(&(SymbolTypeTcc::PacketReceivedLargeDelta as u16)));
    }

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
        assert_eq!(pkt.header().length, 7);
        assert_eq!(pkt.base_sequence_number, 5);
        assert_eq!(pkt.packet_status_count, 7);
        assert_eq!(pkt.reference_time, 5);
        assert_eq!(pkt.fb_pkt_count, 0);
        assert_eq!(pkt.packet_chunks.len(), 1);

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

        let expected_deltas = [
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
        assert_eq!(pkt.recv_deltas.len(), expected_deltas.len());
        for (i, expected) in expected_deltas.iter().enumerate() {
            assert_eq!(&pkt.recv_deltas[i], expected);
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
        assert_eq!(pkt.header().length, 7);
        assert_eq!(pkt.base_sequence_number, 65535);
        assert_eq!(pkt.packet_status_count, 13);
        assert_eq!(pkt.reference_time, 5);
        assert_eq!(pkt.fb_pkt_count, 0);
        assert_eq!(pkt.packet_chunks.len(), 2);

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

        let expected_deltas = [
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
        assert_eq!(pkt.recv_deltas.len(), expected_deltas.len());
        for (i, expected) in expected_deltas.iter().enumerate() {
            assert_eq!(&pkt.recv_deltas[i], expected);
        }
    }

    //"get RTCP"
    {
        let tests = vec![(320, 1, 5, 1), (1000, 2, 15, 2)];
        for (arrival_ts, sequence_number, want_ref_time, want_base_sequence_number) in tests {
            let mut f = Feedback::new(0, 0, 0);
            f.set_base(sequence_number, arrival_ts * 1000);

            let got = f.get_rtcp();
            assert_eq!(got.reference_time, want_ref_time);
            assert_eq!(got.base_sequence_number, want_base_sequence_number);
        }
    }

    Ok(())
}

fn add_run(r: &mut Recorder, sequence_numbers: &[u16], arrival_times: &[i64]) {
    assert_eq!(sequence_numbers.len(), arrival_times.len());

    for i in 0..sequence_numbers.len() {
        r.record(5000, sequence_numbers[i], arrival_times[i]);
    }
}

const TYPE_TCC_DELTA_SCALE_FACTOR: i64 = 250;
const SCALE_FACTOR_REFERENCE_TIME: i64 = 64000;

fn increase_time(arrival_time: &mut i64, increase_amount: i64) -> i64 {
    *arrival_time += increase_amount;
    *arrival_time
}

fn marshal_all(pkts: &[Box<dyn rtcp::packet::Packet + Send + Sync>]) -> Result<()> {
    for pkt in pkts {
        let _ = pkt.marshal()?;
    }
    Ok(())
}

#[test]
fn test_build_feedback_packet() -> Result<()> {
    let mut r = Recorder::new(5000);

    let mut arrival_time = SCALE_FACTOR_REFERENCE_TIME;
    add_run(
        &mut r,
        &[0, 1, 2, 3, 4, 5, 6, 7],
        &[
            SCALE_FACTOR_REFERENCE_TIME,
            increase_time(&mut arrival_time, TYPE_TCC_DELTA_SCALE_FACTOR),
            increase_time(&mut arrival_time, TYPE_TCC_DELTA_SCALE_FACTOR),
            increase_time(&mut arrival_time, TYPE_TCC_DELTA_SCALE_FACTOR),
            increase_time(&mut arrival_time, TYPE_TCC_DELTA_SCALE_FACTOR),
            increase_time(&mut arrival_time, TYPE_TCC_DELTA_SCALE_FACTOR),
            increase_time(&mut arrival_time, TYPE_TCC_DELTA_SCALE_FACTOR),
            increase_time(&mut arrival_time, TYPE_TCC_DELTA_SCALE_FACTOR * 256),
        ],
    );

    let rtcp_packets = r.build_feedback_packet();
    assert_eq!(1, rtcp_packets.len());

    let expected = TransportLayerCc {
        sender_ssrc: 5000,
        media_ssrc: 5000,
        base_sequence_number: 0,
        reference_time: 1,
        fb_pkt_count: 0,
        packet_status_count: 8,
        packet_chunks: vec![
            PacketStatusChunk::RunLengthChunk(RunLengthChunk {
                type_tcc: StatusChunkTypeTcc::RunLengthChunk,
                packet_status_symbol: SymbolTypeTcc::PacketReceivedSmallDelta,
                run_length: 7,
            }),
            PacketStatusChunk::RunLengthChunk(RunLengthChunk {
                type_tcc: StatusChunkTypeTcc::RunLengthChunk,
                packet_status_symbol: SymbolTypeTcc::PacketReceivedLargeDelta,
                run_length: 1,
            }),
        ],
        recv_deltas: vec![
            RecvDelta {
                type_tcc_packet: SymbolTypeTcc::PacketReceivedSmallDelta,
                delta: 0,
            },
            RecvDelta {
                type_tcc_packet: SymbolTypeTcc::PacketReceivedSmallDelta,
                delta: TYPE_TCC_DELTA_SCALE_FACTOR,
            },
            RecvDelta {
                type_tcc_packet: SymbolTypeTcc::PacketReceivedSmallDelta,
                delta: TYPE_TCC_DELTA_SCALE_FACTOR,
            },
            RecvDelta {
                type_tcc_packet: SymbolTypeTcc::PacketReceivedSmallDelta,
                delta: TYPE_TCC_DELTA_SCALE_FACTOR,
            },
            RecvDelta {
                type_tcc_packet: SymbolTypeTcc::PacketReceivedSmallDelta,
                delta: TYPE_TCC_DELTA_SCALE_FACTOR,
            },
            RecvDelta {
                type_tcc_packet: SymbolTypeTcc::PacketReceivedSmallDelta,
                delta: TYPE_TCC_DELTA_SCALE_FACTOR,
            },
            RecvDelta {
                type_tcc_packet: SymbolTypeTcc::PacketReceivedSmallDelta,
                delta: TYPE_TCC_DELTA_SCALE_FACTOR,
            },
            RecvDelta {
                type_tcc_packet: SymbolTypeTcc::PacketReceivedLargeDelta,
                delta: TYPE_TCC_DELTA_SCALE_FACTOR * 256,
            },
        ],
    };

    if let Some(tcc) = rtcp_packets[0].as_any().downcast_ref::<TransportLayerCc>() {
        assert_eq!(tcc, &expected);
    } else {
        panic!();
    }

    marshal_all(&rtcp_packets[..])?;

    Ok(())
}

#[test]
fn test_build_feedback_packet_rolling() -> Result<()> {
    let mut r = Recorder::new(5000);

    let mut arrival_time = SCALE_FACTOR_REFERENCE_TIME;
    add_run(&mut r, &[3], &[arrival_time]);

    let rtcp_packets = r.build_feedback_packet();
    assert_eq!(0, rtcp_packets.len());

    add_run(
        &mut r,
        &[4, 8, 9],
        &[
            increase_time(&mut arrival_time, TYPE_TCC_DELTA_SCALE_FACTOR),
            increase_time(&mut arrival_time, TYPE_TCC_DELTA_SCALE_FACTOR),
            increase_time(&mut arrival_time, TYPE_TCC_DELTA_SCALE_FACTOR),
        ],
    );

    let rtcp_packets = r.build_feedback_packet();
    assert_eq!(rtcp_packets.len(), 1);

    let expected = TransportLayerCc {
        sender_ssrc: 5000,
        media_ssrc: 5000,
        base_sequence_number: 3,
        reference_time: 1,
        fb_pkt_count: 0,
        packet_status_count: 7,
        packet_chunks: vec![PacketStatusChunk::StatusVectorChunk(StatusVectorChunk {
            type_tcc: StatusChunkTypeTcc::StatusVectorChunk,
            symbol_size: SymbolSizeTypeTcc::TwoBit,
            symbol_list: vec![
                SymbolTypeTcc::PacketReceivedSmallDelta,
                SymbolTypeTcc::PacketReceivedSmallDelta,
                SymbolTypeTcc::PacketNotReceived,
                SymbolTypeTcc::PacketNotReceived,
                SymbolTypeTcc::PacketNotReceived,
                SymbolTypeTcc::PacketReceivedSmallDelta,
                SymbolTypeTcc::PacketReceivedSmallDelta,
            ],
        })],
        recv_deltas: vec![
            RecvDelta {
                type_tcc_packet: SymbolTypeTcc::PacketReceivedSmallDelta,
                delta: 0,
            },
            RecvDelta {
                type_tcc_packet: SymbolTypeTcc::PacketReceivedSmallDelta,
                delta: TYPE_TCC_DELTA_SCALE_FACTOR,
            },
            RecvDelta {
                type_tcc_packet: SymbolTypeTcc::PacketReceivedSmallDelta,
                delta: TYPE_TCC_DELTA_SCALE_FACTOR,
            },
            RecvDelta {
                type_tcc_packet: SymbolTypeTcc::PacketReceivedSmallDelta,
                delta: TYPE_TCC_DELTA_SCALE_FACTOR,
            },
        ],
    };

    if let Some(tcc) = rtcp_packets[0].as_any().downcast_ref::<TransportLayerCc>() {
        assert_eq!(tcc, &expected);
    } else {
        panic!();
    }

    marshal_all(&rtcp_packets[..])?;

    Ok(())
}
