use super::*;

use std::io::{BufReader, BufWriter};

use util::Error;

#[test]
fn test_transport_layer_cc_run_length_chunk_unmarshal() -> Result<(), Error> {
    let tests = vec![
        (
            //3.1.3 example1: https://tools.ietf.org/html/draft-holmer-rmcat-transport-wide-cc-extensions-01#page-7
            "example1",
            vec![0, 0xDD],
            RunLengthChunk {
                type_tcc: TypeTCC::RunLengthChunk,
                packet_status_symbol: TypeTCCPacket::NotReceived,
                run_length: 221,
            },
            //None,
        ),
        (
            //3.1.3 example2: https://tools.ietf.org/html/draft-holmer-rmcat-transport-wide-cc-extensions-01#page-7
            "example2",
            vec![0x60, 0x18],
            RunLengthChunk {
                type_tcc: TypeTCC::RunLengthChunk,
                packet_status_symbol: TypeTCCPacket::ReceivedWithoutDelta,
                run_length: 24,
            },
            //None,
        ),
    ];

    for (name, data, want) in tests {
        let mut reader = BufReader::new(data.as_slice());
        let result = RunLengthChunk::unmarshal(&mut reader)?;
        assert_eq!(result, want, "Unmarshal {}: error", name);
    }
    Ok(())
}

#[test]
fn test_transport_layer_cc_run_length_chunk_marshal() -> Result<(), Error> {
    let tests = vec![
        (
            //3.1.3 example1: https://tools.ietf.org/html/draft-holmer-rmcat-transport-wide-cc-extensions-01#page-7
            "example1",
            RunLengthChunk {
                type_tcc: TypeTCC::RunLengthChunk,
                packet_status_symbol: TypeTCCPacket::NotReceived,
                run_length: 221,
            },
            vec![0, 0xDD],
        ),
        (
            //3.1.3 example2: https://tools.ietf.org/html/draft-holmer-rmcat-transport-wide-cc-extensions-01#page-7
            "example2",
            RunLengthChunk {
                type_tcc: TypeTCC::RunLengthChunk,
                packet_status_symbol: TypeTCCPacket::ReceivedWithoutDelta,
                run_length: 24,
            },
            vec![0x60, 0x18],
        ),
    ];
    for (name, chunk, want) in tests {
        let mut data: Vec<u8> = vec![];
        {
            let mut writer = BufWriter::<&mut Vec<u8>>::new(data.as_mut());
            chunk.marshal(&mut writer)?;
        }
        assert_eq!(data, want, "Unmarshal {} error", name);
    }

    Ok(())
}

#[test]
fn test_transport_layer_cc_status_vector_chunk_unmarshal() -> Result<(), Error> {
    let tests = vec![
        (
            //3.1.4 example1: https://tools.ietf.org/html/draft-holmer-rmcat-transport-wide-cc-extensions-01#page-7
            "example1",
            vec![0x9F, 0x1C],
            StatusVectorChunk {
                type_tcc: TypeTCC::StatusVectorChunk,
                symbol_size: TYPE_TCC_SYMBOL_SIZE_ONE_BIT,
                symbol_list: vec![
                    TypeTCCPacket::NotReceived,
                    TypeTCCPacket::ReceivedSmallDelta,
                    TypeTCCPacket::ReceivedSmallDelta,
                    TypeTCCPacket::ReceivedSmallDelta,
                    TypeTCCPacket::ReceivedSmallDelta,
                    TypeTCCPacket::ReceivedSmallDelta,
                    TypeTCCPacket::NotReceived,
                    TypeTCCPacket::NotReceived,
                    TypeTCCPacket::NotReceived,
                    TypeTCCPacket::ReceivedSmallDelta,
                    TypeTCCPacket::ReceivedSmallDelta,
                    TypeTCCPacket::ReceivedSmallDelta,
                    TypeTCCPacket::NotReceived,
                    TypeTCCPacket::NotReceived,
                ],
            },
        ),
        (
            //3.1.4 example2: https://tools.ietf.org/html/draft-holmer-rmcat-transport-wide-cc-extensions-01#page-7
            "example2",
            vec![0xCD, 0x50],
            StatusVectorChunk {
                type_tcc: TypeTCC::StatusVectorChunk,
                symbol_size: TYPE_TCC_SYMBOL_SIZE_TWO_BIT,
                symbol_list: vec![
                    TypeTCCPacket::NotReceived,
                    TypeTCCPacket::ReceivedWithoutDelta,
                    TypeTCCPacket::ReceivedSmallDelta,
                    TypeTCCPacket::ReceivedSmallDelta,
                    TypeTCCPacket::ReceivedSmallDelta,
                    TypeTCCPacket::NotReceived,
                    TypeTCCPacket::NotReceived,
                ],
            },
        ),
    ];

    for (name, data, want) in tests {
        let mut reader = BufReader::new(data.as_slice());
        let result = StatusVectorChunk::unmarshal(&mut reader)?;
        assert_eq!(result, want, "Unmarshal {}", name);
    }

    Ok(())
}

#[test]
fn test_transport_layer_cc_status_vector_chunk_marshal() -> Result<(), Error> {
    let tests = vec![
        (
            //3.1.4 example1: https://tools.ietf.org/html/draft-holmer-rmcat-transport-wide-cc-extensions-01#page-7
            "example1",
            StatusVectorChunk {
                type_tcc: TypeTCC::StatusVectorChunk,
                symbol_size: TYPE_TCC_SYMBOL_SIZE_ONE_BIT,
                symbol_list: vec![
                    TypeTCCPacket::NotReceived,
                    TypeTCCPacket::ReceivedSmallDelta,
                    TypeTCCPacket::ReceivedSmallDelta,
                    TypeTCCPacket::ReceivedSmallDelta,
                    TypeTCCPacket::ReceivedSmallDelta,
                    TypeTCCPacket::ReceivedSmallDelta,
                    TypeTCCPacket::NotReceived,
                    TypeTCCPacket::NotReceived,
                    TypeTCCPacket::NotReceived,
                    TypeTCCPacket::ReceivedSmallDelta,
                    TypeTCCPacket::ReceivedSmallDelta,
                    TypeTCCPacket::ReceivedSmallDelta,
                    TypeTCCPacket::NotReceived,
                    TypeTCCPacket::NotReceived,
                ],
            },
            vec![0x9F, 0x1C],
        ),
        (
            //3.1.4 example2: https://tools.ietf.org/html/draft-holmer-rmcat-transport-wide-cc-extensions-01#page-7
            "example2",
            StatusVectorChunk {
                type_tcc: TypeTCC::StatusVectorChunk,
                symbol_size: TYPE_TCC_SYMBOL_SIZE_TWO_BIT,
                symbol_list: vec![
                    TypeTCCPacket::NotReceived,
                    TypeTCCPacket::ReceivedWithoutDelta,
                    TypeTCCPacket::ReceivedSmallDelta,
                    TypeTCCPacket::ReceivedSmallDelta,
                    TypeTCCPacket::ReceivedSmallDelta,
                    TypeTCCPacket::NotReceived,
                    TypeTCCPacket::NotReceived,
                ],
            },
            vec![0xCD, 0x50],
        ),
    ];

    for (name, chunk, want) in tests {
        let mut data: Vec<u8> = vec![];
        {
            let mut writer = BufWriter::<&mut Vec<u8>>::new(data.as_mut());
            chunk.marshal(&mut writer)?;
        }
        assert_eq!(data, want, "Unmarshal {} error", name);
    }
    Ok(())
}

#[test]
fn test_transport_layer_cc_recv_delta_unmarshal() -> Result<(), Error> {
    let tests = vec![
        (
            "small delta 63.75ms",
            vec![0xFF],
            RecvDelta {
                type_tcc_packet: TypeTCCPacket::ReceivedSmallDelta,
                // 255 * 250
                delta: 63750,
            },
        ),
        (
            "big delta 8191.75ms",
            vec![0x7F, 0xFF],
            RecvDelta {
                type_tcc_packet: TypeTCCPacket::ReceivedLargeDelta,
                // 32767 * 250
                delta: 8191750,
            },
        ),
        (
            "big delta -8192ms",
            vec![0x80, 0x00],
            RecvDelta {
                type_tcc_packet: TypeTCCPacket::ReceivedLargeDelta,
                // -32768 * 250
                delta: -8192000,
            },
        ),
    ];

    for (name, data, want) in tests {
        let mut reader = BufReader::new(data.as_slice());
        let result = RecvDelta::unmarshal(&mut reader)?;
        assert_eq!(result, want, "Unmarshal {}", name);
    }
    Ok(())
}

#[test]
fn test_transport_layer_cc_recv_delta_marshal() -> Result<(), Error> {
    let tests = vec![
        (
            "small delta 63.75ms",
            RecvDelta {
                type_tcc_packet: TypeTCCPacket::ReceivedSmallDelta,
                // 255 * 250
                delta: 63750,
            },
            vec![0xFF],
        ),
        (
            "big delta 8191.75ms",
            RecvDelta {
                type_tcc_packet: TypeTCCPacket::ReceivedLargeDelta,
                // 32767 * 250
                delta: 8191750,
            },
            vec![0x7F, 0xFF],
        ),
        (
            "big delta -8192ms",
            RecvDelta {
                type_tcc_packet: TypeTCCPacket::ReceivedLargeDelta,
                // -32768 * 250
                delta: -8192000,
            },
            vec![0x80, 0x00],
        ),
    ];

    for (name, chunk, want) in tests {
        let mut data: Vec<u8> = vec![];
        {
            let mut writer = BufWriter::<&mut Vec<u8>>::new(data.as_mut());
            chunk.marshal(&mut writer)?;
        }
        assert_eq!(data, want, "Unmarshal {} error", name);
    }

    Ok(())
}

// 0                   1                   2                   3
// 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
// |V=2|P|  FMT=15 |    PT=205     |           length              |
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
// |                     SSRC of packet sender                     |
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
// |                      SSRC of media source                     |
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
// |      base sequence number     |      packet status count      |
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
// |                 reference time                | fb pkt. count |
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
// |         packet chunk          |  recv delta   |  recv delta   |
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
// 0b10101111,0b11001101,0b00000000,0b00000101,
// 0b11111010,0b00010111,0b11111010,0b00010111,
// 0b01000011,0b00000011,0b00101111,0b10100000,
// 0b00000000,0b10011001,0b00000000,0b00000001,
// 0b00111101,0b11101000,0b00000010,0b00010111,
// 0b00100000,0b00000001,0b10010100,0b00000001,

#[test]
fn test_transport_layer_cc_unmarshal() -> Result<(), Error> {
    let tests = vec![
        (
            "example1",
            vec![
                0xaf, 0xcd, 0x0, 0x5, 0xfa, 0x17, 0xfa, 0x17, 0x43, 0x3, 0x2f, 0xa0, 0x0, 0x99,
                0x0, 0x1, 0x3d, 0xe8, 0x2, 0x17, 0x20, 0x1, 0x94, 0x1,
            ],
            TransportLayerCC {
                header: Header {
                    padding: true,
                    count: FORMAT_TCC,
                    packet_type: PacketType::TransportSpecificFeedback,
                    length: 5,
                },
                sender_ssrc: 4195875351,
                media_ssrc: 1124282272,
                base_sequence_number: 153,
                packet_status_count: 1,
                reference_time: 4057090,
                fb_pkt_count: 23,
                // 0b00100000, 0b00000001
                packet_chunks: vec![PacketStatusChunk::RunLengthChunk(RunLengthChunk {
                    type_tcc: TypeTCC::RunLengthChunk,
                    packet_status_symbol: TypeTCCPacket::ReceivedSmallDelta,
                    run_length: 1,
                })],
                // 0b10010100
                recv_deltas: vec![RecvDelta {
                    type_tcc_packet: TypeTCCPacket::ReceivedSmallDelta,
                    delta: 37000,
                }],
            },
        ),
        (
            "example2",
            vec![
                0xaf, 0xcd, 0x0, 0x6, 0xfa, 0x17, 0xfa, 0x17, 0x19, 0x3d, 0xd8, 0xbb, 0x1, 0x74,
                0x0, 0xe, 0x45, 0xb1, 0x5a, 0x40, 0xd8, 0x0, 0xf0, 0xff, 0xd0, 0x0, 0x0, 0x3,
            ],
            TransportLayerCC {
                header: Header {
                    padding: true,
                    count: FORMAT_TCC,
                    packet_type: PacketType::TransportSpecificFeedback,
                    length: 6,
                },
                sender_ssrc: 4195875351,
                media_ssrc: 423483579,
                base_sequence_number: 372,
                packet_status_count: 14,
                reference_time: 4567386,
                fb_pkt_count: 64,
                packet_chunks: vec![
                    PacketStatusChunk::StatusVectorChunk(StatusVectorChunk {
                        type_tcc: TypeTCC::StatusVectorChunk,
                        symbol_size: TYPE_TCC_SYMBOL_SIZE_TWO_BIT,
                        symbol_list: vec![
                            TypeTCCPacket::ReceivedSmallDelta,
                            TypeTCCPacket::ReceivedLargeDelta,
                            TypeTCCPacket::NotReceived,
                            TypeTCCPacket::NotReceived,
                            TypeTCCPacket::NotReceived,
                            TypeTCCPacket::NotReceived,
                            TypeTCCPacket::NotReceived,
                        ],
                    }),
                    PacketStatusChunk::StatusVectorChunk(StatusVectorChunk {
                        type_tcc: TypeTCC::StatusVectorChunk,
                        symbol_size: TYPE_TCC_SYMBOL_SIZE_TWO_BIT,
                        symbol_list: vec![
                            TypeTCCPacket::ReceivedWithoutDelta,
                            TypeTCCPacket::NotReceived,
                            TypeTCCPacket::NotReceived,
                            TypeTCCPacket::ReceivedWithoutDelta,
                            TypeTCCPacket::ReceivedWithoutDelta,
                            TypeTCCPacket::ReceivedWithoutDelta,
                            TypeTCCPacket::ReceivedWithoutDelta,
                        ],
                    }),
                ],
                // 0b10010100
                recv_deltas: vec![
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedSmallDelta,
                        delta: 52000,
                    },
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedLargeDelta,
                        delta: 0,
                    },
                ],
            },
        ),
        (
            "example3",
            vec![
                0xaf, 0xcd, 0x0, 0x7, 0xfa, 0x17, 0xfa, 0x17, 0x19, 0x3d, 0xd8, 0xbb, 0x1, 0x74,
                0x0, 0x6, 0x45, 0xb1, 0x5a, 0x40, 0x40, 0x2, 0x20, 0x04, 0x1f, 0xfe, 0x1f, 0x9a,
                0xd0, 0x0, 0xd0, 0x0,
            ],
            TransportLayerCC {
                header: Header {
                    padding: true,
                    count: FORMAT_TCC,
                    packet_type: PacketType::TransportSpecificFeedback,
                    length: 7,
                },
                sender_ssrc: 4195875351,
                media_ssrc: 423483579,
                base_sequence_number: 372,
                packet_status_count: 6,
                reference_time: 4567386,
                fb_pkt_count: 64,
                packet_chunks: vec![
                    PacketStatusChunk::RunLengthChunk(RunLengthChunk {
                        type_tcc: TypeTCC::RunLengthChunk,
                        packet_status_symbol: TypeTCCPacket::ReceivedLargeDelta,
                        run_length: 2,
                    }),
                    PacketStatusChunk::RunLengthChunk(RunLengthChunk {
                        type_tcc: TypeTCC::RunLengthChunk,
                        packet_status_symbol: TypeTCCPacket::ReceivedSmallDelta,
                        run_length: 4,
                    }),
                ],
                recv_deltas: vec![
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedLargeDelta,
                        delta: 2047500,
                    },
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedLargeDelta,
                        delta: 2022500,
                    },
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedSmallDelta,
                        delta: 52000,
                    },
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedSmallDelta,
                        delta: 0,
                    },
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedSmallDelta,
                        delta: 52000,
                    },
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedSmallDelta,
                        delta: 0,
                    },
                ],
            },
        ),
        (
            "example4",
            vec![
                0xaf, 0xcd, 0x0, 0x7, 0xfa, 0x17, 0xfa, 0x17, 0x19, 0x3d, 0xd8, 0xbb, 0x0, 0x4,
                0x0, 0x7, 0x10, 0x63, 0x6e, 0x1, 0x20, 0x7, 0x4c, 0x24, 0x24, 0x10, 0xc, 0xc, 0x10,
                0x0, 0x0, 0x3,
            ],
            TransportLayerCC {
                header: Header {
                    padding: true,
                    count: FORMAT_TCC,
                    packet_type: PacketType::TransportSpecificFeedback,
                    length: 7,
                },
                sender_ssrc: 4195875351,
                media_ssrc: 423483579,
                base_sequence_number: 4,
                packet_status_count: 7,
                reference_time: 1074030,
                fb_pkt_count: 1,
                packet_chunks: vec![PacketStatusChunk::RunLengthChunk(RunLengthChunk {
                    type_tcc: TypeTCC::RunLengthChunk,
                    packet_status_symbol: TypeTCCPacket::ReceivedSmallDelta,
                    run_length: 7,
                })],
                recv_deltas: vec![
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedSmallDelta,
                        delta: 19000,
                    },
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedSmallDelta,
                        delta: 9000,
                    },
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedSmallDelta,
                        delta: 9000,
                    },
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedSmallDelta,
                        delta: 4000,
                    },
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedSmallDelta,
                        delta: 3000,
                    },
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedSmallDelta,
                        delta: 3000,
                    },
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedSmallDelta,
                        delta: 4000,
                    },
                ],
            },
        ),
        (
            "example5",
            vec![
                0xaf, 0xcd, 0x0, 0x6, 0xfa, 0x17, 0xfa, 0x17, 0x19, 0x3d, 0xd8, 0xbb, 0x0, 0x1,
                0x0, 0xe, 0x10, 0x63, 0x6d, 0x0, 0xba, 0x0, 0x10, 0xc, 0xc, 0x10, 0x0, 0x3,
            ],
            TransportLayerCC {
                header: Header {
                    padding: true,
                    count: FORMAT_TCC,
                    packet_type: PacketType::TransportSpecificFeedback,
                    length: 6,
                },
                sender_ssrc: 4195875351,
                media_ssrc: 423483579,
                base_sequence_number: 1,
                packet_status_count: 14,
                reference_time: 1074029,
                fb_pkt_count: 0,
                packet_chunks: vec![PacketStatusChunk::StatusVectorChunk(StatusVectorChunk {
                    type_tcc: TypeTCC::StatusVectorChunk,
                    symbol_size: 0,
                    symbol_list: vec![
                        TypeTCCPacket::ReceivedSmallDelta,
                        TypeTCCPacket::ReceivedSmallDelta,
                        TypeTCCPacket::ReceivedSmallDelta,
                        TypeTCCPacket::NotReceived,
                        TypeTCCPacket::ReceivedSmallDelta,
                        TypeTCCPacket::NotReceived,
                        TypeTCCPacket::NotReceived,
                        TypeTCCPacket::NotReceived,
                        TypeTCCPacket::NotReceived,
                        TypeTCCPacket::NotReceived,
                        TypeTCCPacket::NotReceived,
                        TypeTCCPacket::NotReceived,
                        TypeTCCPacket::NotReceived,
                        TypeTCCPacket::NotReceived,
                    ],
                })],
                recv_deltas: vec![
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedSmallDelta,
                        delta: 4000,
                    },
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedSmallDelta,
                        delta: 3000,
                    },
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedSmallDelta,
                        delta: 3000,
                    },
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedSmallDelta,
                        delta: 4000,
                    },
                ],
            },
        ),
        (
            "example6",
            vec![
                0xaf, 0xcd, 0x0, 0x7, 0x9b, 0x74, 0xf6, 0x1f, 0x93, 0x71, 0xdc, 0xbc, 0x85, 0x3c,
                0x0, 0x9, 0x63, 0xf9, 0x16, 0xb3, 0xd5, 0x52, 0x0, 0x30, 0x9b, 0xaa, 0x6a, 0xaa,
                0x7b, 0x1, 0x9, 0x1,
            ],
            TransportLayerCC {
                header: Header {
                    padding: true,
                    count: FORMAT_TCC,
                    packet_type: PacketType::TransportSpecificFeedback,
                    length: 7,
                },
                sender_ssrc: 2608133663,
                media_ssrc: 2473712828,
                base_sequence_number: 34108,
                packet_status_count: 9,
                reference_time: 6551830,
                fb_pkt_count: 179,
                packet_chunks: vec![
                    PacketStatusChunk::StatusVectorChunk(StatusVectorChunk {
                        type_tcc: TypeTCC::StatusVectorChunk,
                        symbol_size: TYPE_TCC_SYMBOL_SIZE_TWO_BIT,
                        symbol_list: vec![
                            TypeTCCPacket::ReceivedSmallDelta,
                            TypeTCCPacket::ReceivedSmallDelta,
                            TypeTCCPacket::ReceivedSmallDelta,
                            TypeTCCPacket::ReceivedSmallDelta,
                            TypeTCCPacket::ReceivedSmallDelta,
                            TypeTCCPacket::NotReceived,
                            TypeTCCPacket::ReceivedLargeDelta,
                        ],
                    }),
                    PacketStatusChunk::RunLengthChunk(RunLengthChunk {
                        type_tcc: TypeTCC::RunLengthChunk,
                        packet_status_symbol: TypeTCCPacket::NotReceived,
                        run_length: 48,
                    }),
                ],
                recv_deltas: vec![
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedSmallDelta,
                        delta: 38750,
                    },
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedSmallDelta,
                        delta: 42500,
                    },
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedSmallDelta,
                        delta: 26500,
                    },
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedSmallDelta,
                        delta: 42500,
                    },
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedSmallDelta,
                        delta: 30750,
                    },
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedLargeDelta,
                        delta: 66250,
                    },
                ],
            },
        ),
    ];

    for (name, data, want) in tests {
        let mut reader = BufReader::new(data.as_slice());
        let result = TransportLayerCC::unmarshal(&mut reader)?;
        assert_eq!(result, want, "Unmarshal {}: error", name);
    }

    Ok(())
}

#[test]
fn test_transport_layer_cc_marshal() -> Result<(), Error> {
    let tests = vec![
        (
            "example1",
            TransportLayerCC {
                header: Header {
                    padding: true,
                    count: FORMAT_TCC,
                    packet_type: PacketType::TransportSpecificFeedback,
                    length: 5,
                },
                sender_ssrc: 4195875351,
                media_ssrc: 1124282272,
                base_sequence_number: 153,
                packet_status_count: 1,
                reference_time: 4057090,
                fb_pkt_count: 23,
                // 0b00100000, 0b00000001
                packet_chunks: vec![PacketStatusChunk::RunLengthChunk(RunLengthChunk {
                    type_tcc: TypeTCC::RunLengthChunk,
                    packet_status_symbol: TypeTCCPacket::ReceivedSmallDelta,
                    run_length: 1,
                })],
                // 0b10010100
                recv_deltas: vec![RecvDelta {
                    type_tcc_packet: TypeTCCPacket::ReceivedSmallDelta,
                    delta: 37000,
                }],
            },
            vec![
                0xaf, 0xcd, 0x0, 0x5, 0xfa, 0x17, 0xfa, 0x17, 0x43, 0x3, 0x2f, 0xa0, 0x0, 0x99,
                0x0, 0x1, 0x3d, 0xe8, 0x2, 0x17, 0x20, 0x1, 0x94, 0x1,
            ],
        ),
        (
            "example2",
            TransportLayerCC {
                header: Header {
                    padding: true,
                    count: FORMAT_TCC,
                    packet_type: PacketType::TransportSpecificFeedback,
                    length: 6,
                },
                sender_ssrc: 4195875351,
                media_ssrc: 423483579,
                base_sequence_number: 372,
                packet_status_count: 2,
                reference_time: 4567386,
                fb_pkt_count: 64,
                packet_chunks: vec![
                    PacketStatusChunk::StatusVectorChunk(StatusVectorChunk {
                        type_tcc: TypeTCC::StatusVectorChunk,
                        symbol_size: TYPE_TCC_SYMBOL_SIZE_TWO_BIT,
                        symbol_list: vec![
                            TypeTCCPacket::ReceivedSmallDelta,
                            TypeTCCPacket::ReceivedLargeDelta,
                            TypeTCCPacket::NotReceived,
                            TypeTCCPacket::NotReceived,
                            TypeTCCPacket::NotReceived,
                            TypeTCCPacket::NotReceived,
                            TypeTCCPacket::NotReceived,
                        ],
                    }),
                    PacketStatusChunk::StatusVectorChunk(StatusVectorChunk {
                        type_tcc: TypeTCC::StatusVectorChunk,
                        symbol_size: TYPE_TCC_SYMBOL_SIZE_TWO_BIT,
                        symbol_list: vec![
                            TypeTCCPacket::ReceivedWithoutDelta,
                            TypeTCCPacket::NotReceived,
                            TypeTCCPacket::NotReceived,
                            TypeTCCPacket::ReceivedWithoutDelta,
                            TypeTCCPacket::ReceivedWithoutDelta,
                            TypeTCCPacket::ReceivedWithoutDelta,
                            TypeTCCPacket::ReceivedWithoutDelta,
                        ],
                    }),
                ],
                // 0b10010100
                recv_deltas: vec![
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedSmallDelta,
                        delta: 52000,
                    },
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedLargeDelta,
                        delta: 0,
                    },
                ],
            },
            vec![
                0xaf, 0xcd, 0x0, 0x6, 0xfa, 0x17, 0xfa, 0x17, 0x19, 0x3d, 0xd8, 0xbb, 0x1, 0x74,
                0x0, 0x2, 0x45, 0xb1, 0x5a, 0x40, 0xd8, 0x0, 0xf0, 0xff, 0xd0, 0x0, 0x0, 0x1,
            ],
        ),
        (
            "example3",
            TransportLayerCC {
                header: Header {
                    padding: true,
                    count: FORMAT_TCC,
                    packet_type: PacketType::TransportSpecificFeedback,
                    length: 7,
                },
                sender_ssrc: 4195875351,
                media_ssrc: 423483579,
                base_sequence_number: 372,
                packet_status_count: 6,
                reference_time: 4567386,
                fb_pkt_count: 64,
                packet_chunks: vec![
                    PacketStatusChunk::RunLengthChunk(RunLengthChunk {
                        type_tcc: TypeTCC::RunLengthChunk,
                        packet_status_symbol: TypeTCCPacket::ReceivedLargeDelta,
                        run_length: 2,
                    }),
                    PacketStatusChunk::RunLengthChunk(RunLengthChunk {
                        type_tcc: TypeTCC::RunLengthChunk,
                        packet_status_symbol: TypeTCCPacket::ReceivedSmallDelta,
                        run_length: 4,
                    }),
                ],
                recv_deltas: vec![
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedLargeDelta,
                        delta: 2047500,
                    },
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedLargeDelta,
                        delta: 2022500,
                    },
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedSmallDelta,
                        delta: 52000,
                    },
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedSmallDelta,
                        delta: 0,
                    },
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedSmallDelta,
                        delta: 52000,
                    },
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedSmallDelta,
                        delta: 0,
                    },
                ],
            },
            vec![
                0xaf, 0xcd, 0x0, 0x7, 0xfa, 0x17, 0xfa, 0x17, 0x19, 0x3d, 0xd8, 0xbb, 0x1, 0x74,
                0x0, 0x6, 0x45, 0xb1, 0x5a, 0x40, 0x40, 0x2, 0x20, 0x04, 0x1f, 0xfe, 0x1f, 0x9a,
                0xd0, 0x0, 0xd0, 0x0,
            ],
        ),
        (
            "example4",
            TransportLayerCC {
                header: Header {
                    padding: true,
                    count: FORMAT_TCC,
                    packet_type: PacketType::TransportSpecificFeedback,
                    length: 7,
                },
                sender_ssrc: 4195875351,
                media_ssrc: 423483579,
                base_sequence_number: 4,
                packet_status_count: 7,
                reference_time: 1074030,
                fb_pkt_count: 1,
                packet_chunks: vec![PacketStatusChunk::RunLengthChunk(RunLengthChunk {
                    type_tcc: TypeTCC::RunLengthChunk,
                    packet_status_symbol: TypeTCCPacket::ReceivedSmallDelta,
                    run_length: 7,
                })],
                recv_deltas: vec![
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedSmallDelta,
                        delta: 19000,
                    },
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedSmallDelta,
                        delta: 9000,
                    },
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedSmallDelta,
                        delta: 9000,
                    },
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedSmallDelta,
                        delta: 4000,
                    },
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedSmallDelta,
                        delta: 3000,
                    },
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedSmallDelta,
                        delta: 3000,
                    },
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedSmallDelta,
                        delta: 4000,
                    },
                ],
            },
            vec![
                0xaf, 0xcd, 0x0, 0x7, 0xfa, 0x17, 0xfa, 0x17, 0x19, 0x3d, 0xd8, 0xbb, 0x0, 0x4,
                0x0, 0x7, 0x10, 0x63, 0x6e, 0x1, 0x20, 0x7, 0x4c, 0x24, 0x24, 0x10, 0xc, 0xc, 0x10,
                0x0, 0x0, 0x3,
            ],
        ),
        (
            "example5",
            TransportLayerCC {
                header: Header {
                    padding: true,
                    count: FORMAT_TCC,
                    packet_type: PacketType::TransportSpecificFeedback,
                    length: 6,
                },
                sender_ssrc: 4195875351,
                media_ssrc: 423483579,
                base_sequence_number: 1,
                packet_status_count: 14,
                reference_time: 1074029,
                fb_pkt_count: 0,
                packet_chunks: vec![PacketStatusChunk::StatusVectorChunk(StatusVectorChunk {
                    type_tcc: TypeTCC::StatusVectorChunk,
                    symbol_size: TYPE_TCC_SYMBOL_SIZE_ONE_BIT,
                    symbol_list: vec![
                        TypeTCCPacket::ReceivedSmallDelta,
                        TypeTCCPacket::ReceivedSmallDelta,
                        TypeTCCPacket::ReceivedSmallDelta,
                        TypeTCCPacket::NotReceived,
                        TypeTCCPacket::ReceivedSmallDelta,
                        TypeTCCPacket::NotReceived,
                        TypeTCCPacket::NotReceived,
                        TypeTCCPacket::NotReceived,
                        TypeTCCPacket::NotReceived,
                        TypeTCCPacket::NotReceived,
                        TypeTCCPacket::NotReceived,
                        TypeTCCPacket::NotReceived,
                        TypeTCCPacket::NotReceived,
                        TypeTCCPacket::NotReceived,
                    ],
                })],
                recv_deltas: vec![
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedSmallDelta,
                        delta: 4000,
                    },
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedSmallDelta,
                        delta: 3000,
                    },
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedSmallDelta,
                        delta: 3000,
                    },
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedSmallDelta,
                        delta: 4000,
                    },
                ],
            },
            vec![
                0xaf, 0xcd, 0x0, 0x6, 0xfa, 0x17, 0xfa, 0x17, 0x19, 0x3d, 0xd8, 0xbb, 0x0, 0x1,
                0x0, 0xe, 0x10, 0x63, 0x6d, 0x0, 0xba, 0x0, 0x10, 0xc, 0xc, 0x10, 0x0, 0x2,
            ],
        ),
        (
            "example6",
            TransportLayerCC {
                header: Header {
                    padding: true,
                    count: FORMAT_TCC,
                    packet_type: PacketType::TransportSpecificFeedback,
                    length: 7,
                },
                sender_ssrc: 4195875351,
                media_ssrc: 1124282272,
                base_sequence_number: 39956,
                packet_status_count: 12,
                reference_time: 7701536,
                fb_pkt_count: 0,
                packet_chunks: vec![PacketStatusChunk::StatusVectorChunk(StatusVectorChunk {
                    type_tcc: TypeTCC::StatusVectorChunk,
                    symbol_size: TYPE_TCC_SYMBOL_SIZE_ONE_BIT,
                    symbol_list: vec![
                        TypeTCCPacket::ReceivedSmallDelta,
                        TypeTCCPacket::ReceivedSmallDelta,
                        TypeTCCPacket::ReceivedSmallDelta,
                        TypeTCCPacket::ReceivedSmallDelta,
                        TypeTCCPacket::ReceivedSmallDelta,
                        TypeTCCPacket::NotReceived,
                        TypeTCCPacket::ReceivedSmallDelta,
                        TypeTCCPacket::ReceivedSmallDelta,
                        TypeTCCPacket::NotReceived,
                        TypeTCCPacket::NotReceived,
                        TypeTCCPacket::NotReceived,
                        TypeTCCPacket::NotReceived,
                    ],
                })],
                recv_deltas: vec![
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedSmallDelta,
                        delta: 48250,
                    },
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedSmallDelta,
                        delta: 15750,
                    },
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedSmallDelta,
                        delta: 14750,
                    },
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedSmallDelta,
                        delta: 15750,
                    },
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedSmallDelta,
                        delta: 20750,
                    },
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedSmallDelta,
                        delta: 36000,
                    },
                    RecvDelta {
                        type_tcc_packet: TypeTCCPacket::ReceivedSmallDelta,
                        delta: 14750,
                    },
                ],
            },
            vec![
                0xaf, 0xcd, 0x0, 0x7, 0xfa, 0x17, 0xfa, 0x17, 0x43, 0x3, 0x2f, 0xa0, 0x9c, 0x14,
                0x0, 0xc, 0x75, 0x84, 0x20, 0x0, 0xbe, 0xc0, 0xc1, 0x3f, 0x3b, 0x3f, 0x53, 0x90,
                0x3b, 0x0, 0x0, 0x3,
            ],
        ),
    ];

    for (name, chunk, want) in tests {
        let mut data: Vec<u8> = vec![];
        {
            let mut writer = BufWriter::<&mut Vec<u8>>::new(data.as_mut());
            chunk.marshal(&mut writer)?;
        }
        assert_eq!(data, want, "Unmarshal {} error", name);
    }

    Ok(())
}
