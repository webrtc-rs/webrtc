use super::*;

use std::io::{BufReader, BufWriter};

use util::Error;

#[test]
fn test_source_description_unmarshal() -> Result<(), Error> {
    let tests = vec![
        (
            "nil",
            vec![],
            SourceDescription::default(),
            Some(ERR_FAILED_TO_FILL_WHOLE_BUFFER.clone()),
        ),
        (
            "no chunks",
            vec![
                // v=2, p=0, count=1, SDES, len=8
                0x80, 0xca, 0x00, 0x04,
            ],
            SourceDescription::default(),
            None,
        ),
        (
            "missing type",
            vec![
                // v=2, p=0, count=1, SDES, len=8
                0x81, 0xca, 0x00, 0x08, // ssrc=0x00000000
                0x00, 0x00, 0x00, 0x00,
            ],
            SourceDescription::default(),
            Some(ERR_FAILED_TO_FILL_WHOLE_BUFFER.clone()),
        ),
        (
            "bad cname length",
            vec![
                // v=2, p=0, count=1, SDES, len=10
                0x81, 0xca, 0x00, 0x0a, // ssrc=0x00000000
                0x00, 0x00, 0x00, 0x00, // CNAME, len = 1
                0x01, 0x01,
            ],
            SourceDescription::default(),
            Some(ERR_PACKET_TOO_SHORT.clone()),
        ),
        (
            "short cname",
            vec![
                // v=2, p=0, count=1, SDES, len=9
                0x81, 0xca, 0x00, 0x09, // ssrc=0x00000000
                0x00, 0x00, 0x00, 0x00, // CNAME, Missing length
                0x01,
            ],
            SourceDescription::default(),
            Some(ERR_FAILED_TO_FILL_WHOLE_BUFFER.clone()),
        ),
        (
            "no end",
            vec![
                // v=2, p=0, count=1, SDES, len=11
                0x81, 0xca, 0x00, 0x0b, // ssrc=0x00000000
                0x00, 0x00, 0x00, 0x00, // CNAME, len=1, content=A
                0x01, 0x02, 0x41,
                // Missing END
            ],
            SourceDescription::default(),
            Some(ERR_PACKET_TOO_SHORT.clone()),
        ),
        (
            "bad octet count",
            vec![
                // v=2, p=0, count=1, SDES, len=10
                0x81, 0xca, 0x00, 0x0a, // ssrc=0x00000000
                0x00, 0x00, 0x00, 0x00, // CNAME, len=1
                0x01, 0x01,
            ],
            SourceDescription::default(),
            Some(ERR_PACKET_TOO_SHORT.clone()),
        ),
        (
            "zero item chunk",
            vec![
                // v=2, p=0, count=1, SDES, len=12
                0x81, 0xca, 0x00, 0x0c, // ssrc=0x01020304
                0x01, 0x02, 0x03, 0x04, // END + padding
                0x00, 0x00, 0x00, 0x00,
            ],
            SourceDescription {
                chunks: vec![SourceDescriptionChunk {
                    source: 0x01020304,
                    items: vec![],
                }],
            },
            None,
        ),
        (
            "wrong type",
            vec![
                // v=2, p=0, count=1, SR, len=12
                0x81, 0xc8, 0x00, 0x0c, // ssrc=0x01020304
                0x01, 0x02, 0x03, 0x04, // END + padding
                0x00, 0x00, 0x00, 0x00,
            ],
            SourceDescription::default(),
            Some(ERR_WRONG_TYPE.clone()),
        ),
        (
            "bad count in header",
            vec![
                // v=2, p=0, count=1, SDES, len=12
                0x81, 0xca, 0x00, 0x0c,
            ],
            SourceDescription::default(),
            Some(ERR_FAILED_TO_FILL_WHOLE_BUFFER.clone()),
        ),
        (
            "empty string",
            vec![
                // v=2, p=0, count=1, SDES, len=12
                0x81, 0xca, 0x00, 0x0c, // ssrc=0x01020304
                0x01, 0x02, 0x03, 0x04, // CNAME, len=0
                0x01, 0x00, // END + padding
                0x00, 0x00,
            ],
            SourceDescription {
                chunks: vec![SourceDescriptionChunk {
                    source: 0x01020304,
                    items: vec![SourceDescriptionItem {
                        sdes_type: SDESType::SDESCNAME,
                        text: "".to_string(),
                    }],
                }],
            },
            None,
        ),
        (
            "two items",
            vec![
                // v=2, p=0, count=1, SDES, len=16
                0x81, 0xca, 0x00, 0x10, // ssrc=0x10000000
                0x10, 0x00, 0x00, 0x00, // CNAME, len=1, content=A
                0x01, 0x01, 0x41, // PHONE, len=1, content=B
                0x04, 0x01, 0x42, // END + padding
                0x00, 0x00,
            ],
            SourceDescription {
                chunks: vec![SourceDescriptionChunk {
                    source: 0x10000000,
                    items: vec![
                        SourceDescriptionItem {
                            sdes_type: SDESType::SDESCNAME,
                            text: "A".to_string(),
                        },
                        SourceDescriptionItem {
                            sdes_type: SDESType::SDESPhone,
                            text: "B".to_string(),
                        },
                    ],
                }],
            },
            None,
        ),
        (
            "two chunks",
            vec![
                // v=2, p=0, count=2, SDES, len=24
                0x82, 0xca, 0x00, 0x18, // ssrc=0x01020304
                0x01, 0x02, 0x03, 0x04,
                // Chunk 1
                // CNAME, len=1, content=A
                0x01, 0x01, 0x41, // END
                0x00, // Chunk 2
                // SSRC 0x05060708
                0x05, 0x06, 0x07, 0x08, // CNAME, len=3, content=BCD
                0x01, 0x03, 0x42, 0x43, 0x44, // END
                0x00, 0x00, 0x00,
            ],
            SourceDescription {
                chunks: vec![
                    SourceDescriptionChunk {
                        source: 0x01020304,
                        items: vec![SourceDescriptionItem {
                            sdes_type: SDESType::SDESCNAME,
                            text: "A".to_string(),
                        }],
                    },
                    SourceDescriptionChunk {
                        source: 0x05060708,
                        items: vec![SourceDescriptionItem {
                            sdes_type: SDESType::SDESCNAME,
                            text: "BCD".to_string(),
                        }],
                    },
                ],
            },
            None,
        ),
    ];

    for (name, data, want, want_error) in tests {
        let mut reader = BufReader::new(data.as_slice());
        let result = SourceDescription::unmarshal(&mut reader);
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
fn test_source_description_roundtrip() -> Result<(), Error> {
    let mut too_long_text = String::new();
    for _i in 0..(1 << 8) {
        too_long_text += "x";
    }

    let mut too_many_chunks = vec![];
    for _i in 0..(1 << 5) {
        too_many_chunks.push(SourceDescriptionChunk::default());
    }

    let tests = vec![
        (
            "valid",
            SourceDescription {
                chunks: vec![
                    SourceDescriptionChunk {
                        source: 1,
                        items: vec![SourceDescriptionItem {
                            sdes_type: SDESType::SDESCNAME,
                            text: "test@example.com".to_string(),
                        }],
                    },
                    SourceDescriptionChunk {
                        source: 2,
                        items: vec![
                            SourceDescriptionItem {
                                sdes_type: SDESType::SDESNote,
                                text: "some note".to_string(),
                            },
                            SourceDescriptionItem {
                                sdes_type: SDESType::SDESNote,
                                text: "another note".to_string(),
                            },
                        ],
                    },
                ],
            },
            None,
        ),
        (
            "item without type",
            SourceDescription {
                chunks: vec![SourceDescriptionChunk {
                    source: 1,
                    items: vec![SourceDescriptionItem {
                        sdes_type: SDESType::SDESEnd,
                        text: "test@example.com".to_string(),
                    }],
                }],
            },
            Some(ERR_SDESMISSING_TYPE.clone()),
        ),
        (
            "zero items",
            SourceDescription {
                chunks: vec![SourceDescriptionChunk {
                    source: 1,
                    items: vec![],
                }],
            },
            None,
        ),
        (
            "email item",
            SourceDescription {
                chunks: vec![SourceDescriptionChunk {
                    source: 1,
                    items: vec![SourceDescriptionItem {
                        sdes_type: SDESType::SDESEmail,
                        text: "test@example.com".to_string(),
                    }],
                }],
            },
            None,
        ),
        (
            "empty text",
            SourceDescription {
                chunks: vec![SourceDescriptionChunk {
                    source: 1,
                    items: vec![SourceDescriptionItem {
                        sdes_type: SDESType::SDESCNAME,
                        text: "".to_string(),
                    }],
                }],
            },
            None,
        ),
        (
            "text too long",
            SourceDescription {
                chunks: vec![SourceDescriptionChunk {
                    source: 1,
                    items: vec![SourceDescriptionItem {
                        sdes_type: SDESType::SDESCNAME,
                        text: too_long_text,
                    }],
                }],
            },
            Some(ERR_SDESTEXT_TOO_LONG.clone()),
        ),
        (
            "count overflow",
            SourceDescription {
                chunks: too_many_chunks,
            },
            Some(ERR_TOO_MANY_CHUNKS.clone()),
        ),
    ];

    for (name, sd, marshal_error) in tests {
        let mut data: Vec<u8> = vec![];
        {
            let mut writer = BufWriter::<&mut Vec<u8>>::new(data.as_mut());
            let result = sd.marshal(&mut writer);
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
        let decoded = SourceDescription::unmarshal(&mut reader)?;
        assert_eq!(
            decoded, sd,
            "{} header round trip: got {:?}, want {:?}",
            name, decoded, sd
        )
    }

    Ok(())
}
