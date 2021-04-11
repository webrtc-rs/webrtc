#[cfg(test)]
mod test {
    use crate::{error::Error, full_intra_request::*};

    #[test]
    fn test_full_intra_request_unmarshal() {
        let tests = vec![
            (
                "valid",
                vec![
                    0x84, 0xce, 0x00, 0x03, // v=2, p=0, FMT=4, PSFB, len=3
                    0x00, 0x00, 0x00, 0x00, // ssrc=0x0
                    0x4b, 0xc4, 0xfc, 0xb4, // ssrc=0x4bc4fcb4
                    0x12, 0x34, 0x56, 0x78, // ssrc=0x12345678
                    0x42, 0x00, 0x00, 0x00, // Seqno=0x42
                ],
                FullIntraRequest {
                    sender_ssrc: 0x0,
                    media_ssrc: 0x4bc4fcb4,
                    fir: vec![FirEntry {
                        ssrc: 0x12345678,
                        sequence_number: 0x42,
                    }],
                },
                Ok(()),
            ),
            (
                "also valid",
                vec![
                    0x84, 0xce, 0x00, 0x05, // v=2, p=0, FMT=4, PSFB, len=3
                    0x00, 0x00, 0x00, 0x00, // ssrc=0x0
                    0x4b, 0xc4, 0xfc, 0xb4, // ssrc=0x4bc4fcb4
                    0x12, 0x34, 0x56, 0x78, // ssrc=0x12345678
                    0x42, 0x00, 0x00, 0x00, // Seqno=0x42
                    0x98, 0x76, 0x54, 0x32, // ssrc=0x98765432
                    0x57, 0x00, 0x00, 0x00, // Seqno=0x57
                ],
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
                Ok(()),
            ),
            (
                "packet too short",
                vec![0x00, 0x00, 0x00, 0x00],
                FullIntraRequest::default(),
                Err(Error::PacketTooShort),
            ),
            (
                "invalid header",
                vec![
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                ],
                FullIntraRequest::default(),
                Err(Error::BadVersion),
            ),
            (
                "wrong type",
                vec![
                    0x84, 0xc9, 0x00, 0x03, // v=2, p=0, FMT=4, RR, len=3
                    0x00, 0x00, 0x00, 0x00, // ssrc=0x0
                    0x4b, 0xc4, 0xfc, 0xb4, // ssrc=0x4bc4fcb4
                    0x12, 0x34, 0x56, 0x78, // ssrc=0x12345678
                    0x42, 0x00, 0x00, 0x00, // Seqno=0x42
                ],
                FullIntraRequest::default(),
                Err(Error::WrongType),
            ),
            (
                "wrong fmt",
                vec![
                    0x82, 0xce, 0x00, 0x03, // v=2, p=0, FMT=2, PSFB, len=3
                    0x00, 0x00, 0x00, 0x00, // ssrc=0x0
                    0x4b, 0xc4, 0xfc, 0xb4, // ssrc=0x4bc4fcb4
                    0x12, 0x34, 0x56, 0x78, // ssrc=0x12345678
                    0x42, 0x00, 0x00, 0x00, // Seqno=0x42
                ],
                FullIntraRequest::default(),
                Err(Error::WrongType),
            ),
        ];

        for (name, data, want_fir, want_error) in tests {
            let mut fir = FullIntraRequest::default();
            let got = fir.unmarshal(&mut data.as_slice().into());

            assert_eq!(
                got, want_error,
                "Unmarshal {} rr: err = {:?}, want {:?}",
                name, got, want_error
            );

            assert_eq!(
                fir, want_fir,
                "Unmarshal {} rr: got {:?}, want {:?}",
                name, fir, want_fir
            );
        }
    }

    #[test]
    fn test_full_intra_request_round_trip() {
        let tests: Vec<(&str, FullIntraRequest, Result<(), Error>)> = vec![
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
                Ok(()),
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
                Ok(()),
            ),
        ];

        for (name, fir, marshal_error) in tests {
            let data = fir.marshal();

            assert_eq!(
                data.is_ok(),
                marshal_error.is_ok(),
                "Marshal {}: err = {:?}, want {:?}",
                name,
                data,
                marshal_error
            );

            match data {
                Ok(mut e) => {
                    let mut decoded = FullIntraRequest::default();

                    decoded
                        .unmarshal(&mut e)
                        .expect(format!("Unmarshal {}", name).as_str());

                    assert_eq!(
                        decoded, fir,
                        "{} rr round trip: got {:?}, want {:?}",
                        name, decoded, fir
                    );
                }

                Err(_) => continue,
            }
        }
    }

    #[test]
    fn test_full_intra_request_unmarshal_header() {
        let tests: Vec<(&str, Vec<u8>, Header, Result<(), Error>)> = vec![(
            "valid header",
            vec![
                0x84, 0xce, 0x00, 0x02, // v=2, p=0, FMT=1, PSFB, len=1
                0x00, 0x00, 0x00, 0x00, // ssrc=0x0
                0x4b, 0xc4, 0xfc, 0xb4, 0x00, 0x00, 0x00, 0x00, // ssrc=0x4bc4fcb4
            ],
            Header {
                count: header::FORMAT_FIR,
                packet_type: header::PacketType::PayloadSpecificFeedback,
                length: 2,
                ..Default::default()
            },
            Ok(()),
        )];

        for (name, data, want, want_error) in tests {
            let mut fir = FullIntraRequest::default();

            let data = fir.unmarshal(&mut data.as_slice().into());

            assert_eq!(
                data, want_error,
                "Unmarshal header {} rr: err = {:?}, want {:?}",
                name, data, want_error
            );

            match data {
                Err(_) => continue,

                Ok(_) => {
                    let h = fir.header();

                    assert_eq!(
                        h, want,
                        "Unmarshal header {} rr: got {:?}, want {:?}",
                        name, h, want
                    )
                }
            }
        }
    }
}
