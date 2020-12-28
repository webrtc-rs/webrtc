#[cfg(test)]
mod test {
    use crate::{errors::Error, transport_layer_nack::*};

    #[test]
    fn test_transport_layer_nack_unmarshal() {
        let tests = vec![
            (
                "valid",
                vec![
                    // TransportLayerNack
                    0x81, 0xcd, 0x0, 0x3, // sender=0x902f9e2e
                    0x90, 0x2f, 0x9e, 0x2e, // media=0x902f9e2e
                    0x90, 0x2f, 0x9e, 0x2e, // nack 0xAAAA, 0x5555
                    0xaa, 0xaa, 0x55, 0x55,
                ],
                TransportLayerNack {
                    sender_ssrc: 0x902f9e2e,
                    media_ssrc: 0x902f9e2e,
                    nacks: vec![NackPair {
                        packet_id: 0xaaaa,
                        lost_packets: 0x5555,
                    }],
                },
                None,
            ),
            (
                "short report",
                vec![
                    0x81, 0xcd, 0x0, 0x2, // ssrc=0x902f9e2e
                    0x90, 0x2f, 0x9e, 0x2e,
                    // report ends early
                ],
                TransportLayerNack::default(),
                Some(Error::PacketTooShort),
            ),
            (
                "wrong type",
                vec![
                    // v=2, p=0, count=1, SR, len=7
                    0x81, 0xc8, 0x0, 0x7, // ssrc=0x902f9e2e
                    0x90, 0x2f, 0x9e, 0x2e, // ssrc=0xbc5e9a40
                    0xbc, 0x5e, 0x9a, 0x40, // fracLost=0, totalLost=0
                    0x0, 0x0, 0x0, 0x0, // lastSeq=0x46e1
                    0x0, 0x0, 0x46, 0xe1, // jitter=273
                    0x0, 0x0, 0x1, 0x11, // lsr=0x9f36432
                    0x9, 0xf3, 0x64, 0x32, // delay=150137
                    0x0, 0x2, 0x4a, 0x79,
                ],
                TransportLayerNack::default(),
                Some(Error::WrongType),
            ),
            (
                "nil",
                vec![],
                TransportLayerNack::default(),
                Some(Error::PacketTooShort),
            ),
        ];

        for (name, data, want, want_error) in tests {
            let mut tln = TransportLayerNack::default();

            let result = tln.unmarshal(&mut data[..].into());

            assert_eq!(
                result.clone().err(),
                want_error,
                "Unmarshal {} : got = {:#?}, want {:#?}",
                name,
                result,
                want_error
            );

            match result {
                Ok(_) => {
                    assert_eq!(
                        tln, want,
                        "Unmarshal {} rr: got {:?}, want {:?}",
                        name, tln, want
                    )
                }
                Err(_) => continue,
            }
        }
    }

    #[test]
    fn test_transport_layer_nack_roundtrip() {
        let tests: Vec<(&str, TransportLayerNack, Option<Error>)> = vec![(
            "valid",
            TransportLayerNack {
                sender_ssrc: 0x902f9e2e,
                media_ssrc: 0x902f9e2e,
                nacks: vec![
                    NackPair {
                        packet_id: 1,
                        lost_packets: 0xAA,
                    },
                    NackPair {
                        packet_id: 1034,
                        lost_packets: 0x05,
                    },
                ],
            },
            None,
        )];

        for (name, report, marshal_error) in tests {
            let data = report.marshal();

            assert_eq!(
                data.clone().err(),
                marshal_error,
                "Marshal {}: err = {:?}, want {:?}",
                name,
                data,
                marshal_error
            );

            match data {
                Ok(mut e) => {
                    let mut decoded = TransportLayerNack::default();

                    decoded.unmarshal(&mut e).expect("Unmarshal error");

                    assert_eq!(
                        decoded, report,
                        "{} tln round trip: got {:#?}, want {:#?}",
                        name, decoded, report
                    )
                }

                Err(_) => continue,
            }
        }
    }

    #[test]
    fn test_nack_pair() {
        let test_nack = |s: Vec<u16>, n: NackPair| {
            let l = n.packet_list();

            assert_eq!(s, l, "{:?}: expected {:?}, got {:?}", n, s, l);
        };

        test_nack(
            vec![42],
            NackPair {
                packet_id: 42,
                lost_packets: 0,
            },
        );

        test_nack(
            vec![42, 43],
            NackPair {
                packet_id: 42,
                lost_packets: 1,
            },
        );

        test_nack(
            vec![42, 44],
            NackPair {
                packet_id: 42,
                lost_packets: 2,
            },
        );

        test_nack(
            vec![42, 43, 44],
            NackPair {
                packet_id: 42,
                lost_packets: 3,
            },
        );

        test_nack(
            vec![42, 42 + 16],
            NackPair {
                packet_id: 42,
                lost_packets: 0x8000,
            },
        );
    }

    #[test]
    fn test_transport_layer_nack_pair_generation() {
        let test = vec![
            ("No Sequence Numbers", vec![], vec![]),
            (
                "Single Sequence Number",
                vec![100u16],
                vec![NackPair {
                    packet_id: 100,
                    lost_packets: 0x0,
                }],
            ),
            (
                "Multiple in range, Single NACKPair",
                vec![100, 101, 105, 115],
                vec![NackPair {
                    packet_id: 100,
                    lost_packets: 0x4011,
                }],
            ),
            (
                "Multiple Ranges, Multiple NACKPair",
                vec![100, 117, 500, 501, 502],
                vec![
                    NackPair {
                        packet_id: 100,
                        lost_packets: 0,
                    },
                    NackPair {
                        packet_id: 117,
                        lost_packets: 0,
                    },
                    NackPair {
                        packet_id: 500,
                        lost_packets: 0x3,
                    },
                ],
            ),
        ];

        for (name, seq_numbers, expected) in test {
            let actual = nack_pairs_from_sequence_numbers(&seq_numbers);

            assert_eq!(
                actual, expected,
                "{} NackPair generation mismatch: got {:#?}, want {:#?}",
                name, actual, expected
            )
        }
    }
}
