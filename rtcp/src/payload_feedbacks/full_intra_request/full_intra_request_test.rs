use bytes::Bytes;

use super::*;

#[test]
fn test_full_intra_request_unmarshal() {
    let tests = vec![
        (
            "valid",
            Bytes::from_static(&[
                0x84, 0xce, 0x00, 0x03, // v=2, p=0, FMT=4, PSFB, len=3
                0x00, 0x00, 0x00, 0x00, // ssrc=0x0
                0x4b, 0xc4, 0xfc, 0xb4, // ssrc=0x4bc4fcb4
                0x12, 0x34, 0x56, 0x78, // ssrc=0x12345678
                0x42, 0x00, 0x00, 0x00, // Seqno=0x42
            ]),
            FullIntraRequest {
                sender_ssrc: 0x0,
                media_ssrc: 0x4bc4fcb4,
                fir: vec![FirEntry {
                    ssrc: 0x12345678,
                    sequence_number: 0x42,
                }],
            },
            None,
        ),
        (
            "also valid",
            Bytes::from_static(&[
                0x84, 0xce, 0x00, 0x05, // v=2, p=0, FMT=4, PSFB, len=3
                0x00, 0x00, 0x00, 0x00, // ssrc=0x0
                0x4b, 0xc4, 0xfc, 0xb4, // ssrc=0x4bc4fcb4
                0x12, 0x34, 0x56, 0x78, // ssrc=0x12345678
                0x42, 0x00, 0x00, 0x00, // Seqno=0x42
                0x98, 0x76, 0x54, 0x32, // ssrc=0x98765432
                0x57, 0x00, 0x00, 0x00, // Seqno=0x57
            ]),
            FullIntraRequest {
                sender_ssrc: 0x0,
                media_ssrc: 0x4bc4fcb4,
                fir: vec![
                    FirEntry {
                        ssrc: 0x12345678,
                        sequence_number: 0x42,
                    },
                    FirEntry {
                        ssrc: 0x98765432,
                        sequence_number: 0x57,
                    },
                ],
            },
            None,
        ),
        (
            "packet too short",
            Bytes::from_static(&[0x00, 0x00, 0x00, 0x00]),
            FullIntraRequest::default(),
            Some(Error::PacketTooShort),
        ),
        (
            "invalid header",
            Bytes::from_static(&[
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            ]),
            FullIntraRequest::default(),
            Some(Error::BadVersion),
        ),
        (
            "wrong type",
            Bytes::from_static(&[
                0x84, 0xc9, 0x00, 0x03, // v=2, p=0, FMT=4, RR, len=3
                0x00, 0x00, 0x00, 0x00, // ssrc=0x0
                0x4b, 0xc4, 0xfc, 0xb4, // ssrc=0x4bc4fcb4
                0x12, 0x34, 0x56, 0x78, // ssrc=0x12345678
                0x42, 0x00, 0x00, 0x00, // Seqno=0x42
            ]),
            FullIntraRequest::default(),
            Some(Error::WrongType),
        ),
        (
            "wrong fmt",
            Bytes::from_static(&[
                0x82, 0xce, 0x00, 0x03, // v=2, p=0, FMT=2, PSFB, len=3
                0x00, 0x00, 0x00, 0x00, // ssrc=0x0
                0x4b, 0xc4, 0xfc, 0xb4, // ssrc=0x4bc4fcb4
                0x12, 0x34, 0x56, 0x78, // ssrc=0x12345678
                0x42, 0x00, 0x00, 0x00, // Seqno=0x42
            ]),
            FullIntraRequest::default(),
            Some(Error::WrongType),
        ),
    ];

    for (name, mut data, want, want_error) in tests {
        let got = FullIntraRequest::unmarshal(&mut data);

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
fn test_full_intra_request_round_trip() {
    let tests: Vec<(&str, FullIntraRequest, Option<Error>)> = vec![
        (
            "valid",
            FullIntraRequest {
                sender_ssrc: 1,
                media_ssrc: 2,
                fir: vec![FirEntry {
                    ssrc: 3,
                    sequence_number: 42,
                }],
            },
            None,
        ),
        (
            "also valid",
            FullIntraRequest {
                sender_ssrc: 5000,
                media_ssrc: 6000,
                fir: vec![FirEntry {
                    ssrc: 3,
                    sequence_number: 57,
                }],
            },
            None,
        ),
    ];

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
            let actual = FullIntraRequest::unmarshal(&mut data)
                .unwrap_or_else(|_| panic!("Unmarshal {name}"));

            assert_eq!(
                actual, want,
                "{name} round trip: got {actual:?}, want {want:?}"
            )
        }
    }
}

#[test]
fn test_full_intra_request_unmarshal_header() {
    let tests = vec![(
        "valid header",
        Bytes::from_static(&[
            0x84, 0xce, 0x00, 0x02, // v=2, p=0, FMT=1, PSFB, len=1
            0x00, 0x00, 0x00, 0x00, // ssrc=0x0
            0x4b, 0xc4, 0xfc, 0xb4, 0x00, 0x00, 0x00, 0x00, // ssrc=0x4bc4fcb4
        ]),
        Header {
            count: FORMAT_FIR,
            packet_type: PacketType::PayloadSpecificFeedback,
            length: 2,
            ..Default::default()
        },
    )];

    for (name, mut data, want) in tests {
        let result = FullIntraRequest::unmarshal(&mut data);

        assert!(
            result.is_ok(),
            "Unmarshal header {name} rr: want {result:?}",
        );

        match result {
            Err(_) => continue,

            Ok(fir) => {
                let h = fir.header();

                assert_eq!(
                    h, want,
                    "Unmarshal header {name} rr: got {h:?}, want {want:?}"
                )
            }
        }
    }
}
