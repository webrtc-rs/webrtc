use super::*;

use std::io::{BufReader, BufWriter};

use util::Error;

#[test]
fn test_full_intra_request_unmarshal() -> Result<(), Error> {
    let tests = vec![
        (
            "valid",
            vec![
                // v=2, p=0, FMT=4, PSFB, len=3
                0x84, 0xce, 0x00, 0x03, // ssrc=0x0
                0x00, 0x00, 0x00, 0x00, // ssrc=0x4bc4fcb4
                0x4b, 0xc4, 0xfc, 0xb4, // ssrc=0x12345678
                0x12, 0x34, 0x56, 0x78, // Seqno=0x42
                0x42, 0x00, 0x00, 0x00,
            ],
            FullIntraRequest {
                sender_ssrc: 0x0,
                media_ssrc: 0x4bc4fcb4,
                fir: vec![FIREntry {
                    ssrc: 0x12345678,
                    sequence_number: 0x42,
                }],
            },
            None,
        ),
        (
            "also valid",
            vec![
                // v=2, p=0, FMT=4, PSFB, len=3
                0x84, 0xce, 0x00, 0x05, // ssrc=0x0
                0x00, 0x00, 0x00, 0x00, // ssrc=0x4bc4fcb4
                0x4b, 0xc4, 0xfc, 0xb4, // ssrc=0x12345678
                0x12, 0x34, 0x56, 0x78, // Seqno=0x42
                0x42, 0x00, 0x00, 0x00, // ssrc=0x98765432
                0x98, 0x76, 0x54, 0x32, // Seqno=0x57
                0x57, 0x00, 0x00, 0x00,
            ],
            FullIntraRequest {
                sender_ssrc: 0x0,
                media_ssrc: 0x4bc4fcb4,
                fir: vec![
                    FIREntry {
                        ssrc: 0x12345678,
                        sequence_number: 0x42,
                    },
                    FIREntry {
                        ssrc: 0x98765432,
                        sequence_number: 0x57,
                    },
                ],
            },
            None,
        ),
        (
            "packet too short",
            vec![0x00, 0x00, 0x00, 0x00],
            FullIntraRequest::default(),
            Some(ERR_BAD_VERSION.clone()),
        ),
        (
            "invalid header",
            vec![
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            ],
            FullIntraRequest::default(),
            Some(ERR_BAD_VERSION.clone()),
        ),
        (
            "wrong type",
            vec![
                // v=2, p=0, FMT=4, RR, len=3
                0x84, 0xc9, 0x00, 0x03, // ssrc=0x0
                0x00, 0x00, 0x00, 0x00, // ssrc=0x4bc4fcb4
                0x4b, 0xc4, 0xfc, 0xb4, // ssrc=0x12345678
                0x12, 0x34, 0x56, 0x78, // Seqno=0x42
                0x42, 0x00, 0x00, 0x00,
            ],
            FullIntraRequest::default(),
            Some(ERR_WRONG_TYPE.clone()),
        ),
        (
            "wrong fmt",
            vec![
                // v=2, p=0, FMT=2, PSFB, len=3
                0x82, 0xce, 0x00, 0x03, // ssrc=0x0
                0x00, 0x00, 0x00, 0x00, // ssrc=0x4bc4fcb4
                0x4b, 0xc4, 0xfc, 0xb4, // ssrc=0x12345678
                0x12, 0x34, 0x56, 0x78, // Seqno=0x42
                0x42, 0x00, 0x00, 0x00,
            ],
            FullIntraRequest::default(),
            Some(ERR_WRONG_TYPE.clone()),
        ),
    ];

    for (name, data, want, want_error) in tests {
        let mut reader = BufReader::new(data.as_slice());
        let result = FullIntraRequest::unmarshal(&mut reader);
        if let Some(err) = want_error {
            if let Err(got) = result {
                assert_eq!(
                    got, err,
                    "Unmarshal {} header: err = {}, want {}",
                    name, got, err
                );
            } else {
                assert!(false, "want error in test {}", name);
            }
        } else {
            if let Ok(got) = result {
                assert_eq!(
                    got, want,
                    "Unmarshal {} header: got {:?}, want {:?}",
                    name, got, want,
                )
            } else {
                assert!(false, "must no error in test {}", name);
            }
        }
    }

    Ok(())
}

#[test]
fn test_full_intra_request_round_trip() -> Result<(), Error> {
    let tests = vec![
        (
            "valid",
            FullIntraRequest {
                sender_ssrc: 1,
                media_ssrc: 2,
                fir: vec![FIREntry {
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
                fir: vec![FIREntry {
                    ssrc: 3,
                    sequence_number: 57,
                }],
            },
            None,
        ),
    ];

    for (name, fir, marshal_error) in tests {
        let mut data: Vec<u8> = vec![];
        {
            let mut writer = BufWriter::<&mut Vec<u8>>::new(data.as_mut());
            let result = fir.marshal(&mut writer);
            if let Some(err) = marshal_error {
                if let Err(got) = result {
                    assert_eq!(
                        got, err,
                        "marshal {} header: err = {}, want {}",
                        name, got, err
                    );
                } else {
                    assert!(false, "want error in test {}", name);
                }
                continue;
            } else {
                assert!(result.is_ok(), "must no error in test {}", name);
            }
        }

        let mut reader = BufReader::new(data.as_slice());
        let decoded = FullIntraRequest::unmarshal(&mut reader)?;
        assert_eq!(
            decoded, fir,
            "{} header round trip: got {:?}, want {:?}",
            name, decoded, fir
        )
    }
    Ok(())
}

#[test]
fn test_full_intra_request_unmarshal_header() -> Result<(), Error> {
    let tests = vec![(
        "valid header",
        vec![
            // v=2, p=0, FMT=1, PSFB, len=1
            0x84, 0xce, 0x00, 0x02, // ssrc=0x0
            0x00, 0x00, 0x00, 0x00, // ssrc=0x4bc4fcb4
            0x4b, 0xc4, 0xfc, 0xb4, 0x00, 0x00, 0x00, 0x00,
        ],
        Header {
            count: FORMAT_FIR,
            packet_type: PacketType::PayloadSpecificFeedback,
            length: 2,
            ..Default::default()
        },
        None,
    )];

    for (name, data, want, want_error) in tests {
        let mut reader = BufReader::new(data.as_slice());
        let result = FullIntraRequest::unmarshal(&mut reader);
        if let Some(err) = want_error {
            if let Err(got) = result {
                assert_eq!(
                    got, err,
                    "Unmarshal {} header: err = {}, want {}",
                    name, got, err
                );
            } else {
                assert!(false, "want error in test {}", name);
            }
        } else {
            if let Ok(got) = result {
                assert_eq!(
                    got.header(),
                    want,
                    "Unmarshal {} header: got {:?}, want {:?}",
                    name,
                    got,
                    want,
                )
            } else {
                assert!(false, "must no error in test {}", name);
            }
        }
    }

    Ok(())
}
