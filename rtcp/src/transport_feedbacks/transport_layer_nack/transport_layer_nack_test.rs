use std::sync::{Arc, Mutex};

use bytes::Bytes;

use super::*;

#[test]
fn test_transport_layer_nack_unmarshal() {
    let tests = vec![
        (
            "valid",
            Bytes::from_static(&[
                // TransportLayerNack
                0x81, 0xcd, 0x0, 0x3, // sender=0x902f9e2e
                0x90, 0x2f, 0x9e, 0x2e, // media=0x902f9e2e
                0x90, 0x2f, 0x9e, 0x2e, // nack 0xAAAA, 0x5555
                0xaa, 0xaa, 0x55, 0x55,
            ]),
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
            Bytes::from_static(&[
                0x81, 0xcd, 0x0, 0x2, // ssrc=0x902f9e2e
                0x90, 0x2f, 0x9e, 0x2e,
                // report ends early
            ]),
            TransportLayerNack::default(),
            Some(Error::PacketTooShort),
        ),
        (
            "wrong type",
            Bytes::from_static(&[
                // v=2, p=0, count=1, SR, len=7
                0x81, 0xc8, 0x0, 0x7, // ssrc=0x902f9e2e
                0x90, 0x2f, 0x9e, 0x2e, // ssrc=0xbc5e9a40
                0xbc, 0x5e, 0x9a, 0x40, // fracLost=0, totalLost=0
                0x0, 0x0, 0x0, 0x0, // lastSeq=0x46e1
                0x0, 0x0, 0x46, 0xe1, // jitter=273
                0x0, 0x0, 0x1, 0x11, // lsr=0x9f36432
                0x9, 0xf3, 0x64, 0x32, // delay=150137
                0x0, 0x2, 0x4a, 0x79,
            ]),
            TransportLayerNack::default(),
            Some(Error::WrongType),
        ),
        (
            "nil",
            Bytes::from_static(&[]),
            TransportLayerNack::default(),
            Some(Error::PacketTooShort),
        ),
    ];

    for (name, mut data, want, want_error) in tests {
        let got = TransportLayerNack::unmarshal(&mut data);

        assert_eq!(
            got.is_err(),
            want_error.is_some(),
            "Unmarshal {name} rr: err = {got:?}, want {want_error:?}"
        );

        if let Some(err) = want_error {
            let got_err = got.err().unwrap();
            assert_eq!(
                err, got_err,
                "Unmarshal {name} rr: err = {got_err:?}, want {err:?}",
            );
        } else {
            let actual = got.unwrap();
            assert_eq!(
                actual, want,
                "Unmarshal {name} rr: got {actual:?}, want {want:?}"
            );
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

    for (name, want, want_error) in tests {
        let got = want.marshal();

        assert_eq!(
            got.is_ok(),
            want_error.is_none(),
            "Marshal {name}: err = {got:?}, want {want_error:?}"
        );

        if let Some(err) = want_error {
            let got_err = got.err().unwrap();
            assert_eq!(
                err, got_err,
                "Unmarshal {name} rr: err = {got_err:?}, want {err:?}",
            );
        } else {
            let mut data = got.ok().unwrap();
            let actual = TransportLayerNack::unmarshal(&mut data)
                .unwrap_or_else(|_| panic!("Unmarshal {name}"));

            assert_eq!(
                actual, want,
                "{name} round trip: got {actual:?}, want {want:?}"
            )
        }
    }
}

#[test]
fn test_nack_pair() {
    let test_nack = |s: Vec<u16>, n: NackPair| {
        let l = n.packet_list();

        assert_eq!(s, l, "{n:?}: expected {s:?}, got {l:?}");
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

    // Wrap around
    test_nack(
        vec![65534, 65535, 0, 1],
        NackPair {
            packet_id: 65534,
            lost_packets: 0b0000_0111,
        },
    );

    // Gap
    test_nack(
        vec![123, 125, 127, 129],
        NackPair {
            packet_id: 123,
            lost_packets: 0b0010_1010,
        },
    );
}

#[test]
fn test_nack_pair_range() {
    let n = NackPair {
        packet_id: 42,
        lost_packets: 2,
    };

    let out = Arc::new(Mutex::new(vec![]));
    let out1 = Arc::clone(&out);
    n.range(move |s: u16| -> bool {
        let out2 = Arc::clone(&out1);
        let mut o = out2.lock().unwrap();
        o.push(s);
        true
    });

    {
        let o = out.lock().unwrap();
        assert_eq!(*o, &[42, 44]);
    }

    let out = Arc::new(Mutex::new(vec![]));
    let out1 = Arc::clone(&out);
    n.range(move |s: u16| -> bool {
        let out2 = Arc::clone(&out1);
        let mut o = out2.lock().unwrap();
        o.push(s);
        false
    });

    {
        let o = out.lock().unwrap();
        assert_eq!(*o, &[42]);
    }
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
        // Make sure it doesn't crash.
        (
            "Single Sequence Number (duplicates)",
            vec![100u16, 100],
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
        (
            "Multiple Ranges, Multiple NACKPair (with rollover)",
            vec![100, 117, 65534, 65535, 0, 1, 99],
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
                    packet_id: 65534,
                    lost_packets: 1,
                },
                NackPair {
                    packet_id: 0,
                    lost_packets: 1,
                },
                NackPair {
                    packet_id: 99,
                    lost_packets: 0,
                },
            ],
        ),
    ];

    for (name, seq_numbers, expected) in test {
        let actual = nack_pairs_from_sequence_numbers(&seq_numbers);

        assert_eq!(
            actual, expected,
            "{name} NackPair generation mismatch: got {actual:#?}, want {expected:#?}"
        )
    }
}

/// This test case reproduced a bug in the implementation
#[test]
fn test_lost_packets_is_reset_when_crossing_16_bit_boundary() {
    let seq: Vec<_> = (0u16..=17u16).collect();
    assert_eq!(
        nack_pairs_from_sequence_numbers(&seq),
        vec![
            NackPair {
                packet_id: 0,
                lost_packets: 0b1111_1111_1111_1111,
            },
            NackPair {
                packet_id: 17,
                // Was 0xffff before fixing the bug
                lost_packets: 0b0000_0000_0000_0000,
            }
        ],
    )
}
