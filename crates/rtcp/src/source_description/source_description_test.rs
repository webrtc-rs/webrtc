#[cfg(test)]
mod test {
    use crate::{error::Error, packet::Packet, source_description::*};

    #[test]
    fn test_source_description_unmarshal() {
        let tests = vec![
            (
                "nil",
                vec![],
                SourceDescription::default(),
                Some(Error::PacketTooShort),
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
                Some(Error::PacketTooShort),
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
                Some(Error::PacketTooShort),
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
                Some(Error::PacketTooShort),
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
                Some(Error::PacketTooShort),
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
                Some(Error::PacketTooShort),
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
                Some(Error::WrongType),
            ),
            (
                "bad count in header",
                vec![
                    // v=2, p=0, count=1, SDES, len=12
                    0x81, 0xca, 0x00, 0x0c,
                ],
                SourceDescription::default(),
                Some(Error::InvalidHeader),
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
            let mut sdes = SourceDescription::default();

            let result = sdes.unmarshal(&mut data[..].into());

            assert_eq!(
                result.clone().err(),
                want_error,
                "Unmarshal {}: err = {:?}, want {:?}",
                name,
                result,
                want_error
            );

            match result {
                Ok(_) => {
                    assert_eq!(
                        sdes, want,
                        "Unmarshal {}: got {:#?}, want {:#?}",
                        name, sdes, want
                    );
                }

                Err(_) => continue,
            }
        }
    }

    #[test]
    fn test_source_description_roundtrip() {
        let mut too_long_text = String::new();
        for _ in 0..(1 << 8) {
            too_long_text += "x";
        }

        let mut too_many_chunks = vec![];
        for _ in 0..(1 << 5) {
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
                Some(Error::SDESMissingType),
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
                Some(Error::SDESTextTooLong),
            ),
            (
                "count overflow",
                SourceDescription {
                    chunks: too_many_chunks,
                },
                Some(Error::TooManyChunks),
            ),
        ];

        for (name, sd, marshal_error) in tests {
            let result = sd.marshal();

            assert_eq!(
                result.clone().err(),
                marshal_error,
                "Marshal {}: err = {:?}, want {:?}",
                name,
                result,
                marshal_error
            );

            match result {
                Ok(mut e) => {
                    let mut decoded = SourceDescription::default();

                    decoded
                        .unmarshal(&mut e)
                        .expect(format!("Unmarshal {}", name).as_str());

                    assert_eq!(
                        decoded, sd,
                        "{} sdes round trip: got {:#?}, want {:#?}",
                        name, decoded, sd
                    )
                }

                Err(_) => continue,
            }
        }
    }
}
