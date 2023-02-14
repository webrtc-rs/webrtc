use super::*;

#[test]
fn test_source_description_unmarshal() {
    let tests = vec![
        (
            "nil",
            Bytes::from_static(&[]),
            SourceDescription::default(),
            Some(Error::PacketTooShort),
        ),
        (
            "no chunks",
            Bytes::from_static(&[
                // v=2, p=0, count=1, SDES, len=8
                0x80, 0xca, 0x00, 0x04,
            ]),
            SourceDescription::default(),
            None,
        ),
        (
            "missing type",
            Bytes::from_static(&[
                // v=2, p=0, count=1, SDES, len=8
                0x81, 0xca, 0x00, 0x08, // ssrc=0x00000000
                0x00, 0x00, 0x00, 0x00,
            ]),
            SourceDescription::default(),
            Some(Error::PacketTooShort),
        ),
        (
            "bad cname length",
            Bytes::from_static(&[
                // v=2, p=0, count=1, SDES, len=10
                0x81, 0xca, 0x00, 0x0a, // ssrc=0x00000000
                0x00, 0x00, 0x00, 0x00, // CNAME, len = 1
                0x01, 0x01,
            ]),
            SourceDescription::default(),
            Some(Error::PacketTooShort),
        ),
        (
            "short cname",
            Bytes::from_static(&[
                // v=2, p=0, count=1, SDES, len=9
                0x81, 0xca, 0x00, 0x09, // ssrc=0x00000000
                0x00, 0x00, 0x00, 0x00, // CNAME, Missing length
                0x01,
            ]),
            SourceDescription::default(),
            Some(Error::PacketTooShort),
        ),
        (
            "no end",
            Bytes::from_static(&[
                // v=2, p=0, count=1, SDES, len=11
                0x81, 0xca, 0x00, 0x0b, // ssrc=0x00000000
                0x00, 0x00, 0x00, 0x00, // CNAME, len=1, content=A
                0x01, 0x02, 0x41,
                // Missing END
            ]),
            SourceDescription::default(),
            Some(Error::PacketTooShort),
        ),
        (
            "bad octet count",
            Bytes::from_static(&[
                // v=2, p=0, count=1, SDES, len=10
                0x81, 0xca, 0x00, 0x0a, // ssrc=0x00000000
                0x00, 0x00, 0x00, 0x00, // CNAME, len=1
                0x01, 0x01,
            ]),
            SourceDescription::default(),
            Some(Error::PacketTooShort),
        ),
        (
            "zero item chunk",
            Bytes::from_static(&[
                // v=2, p=0, count=1, SDES, len=12
                0x81, 0xca, 0x00, 0x0c, // ssrc=0x01020304
                0x01, 0x02, 0x03, 0x04, // END + padding
                0x00, 0x00, 0x00, 0x00,
            ]),
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
            Bytes::from_static(&[
                // v=2, p=0, count=1, SR, len=12
                0x81, 0xc8, 0x00, 0x0c, // ssrc=0x01020304
                0x01, 0x02, 0x03, 0x04, // END + padding
                0x00, 0x00, 0x00, 0x00,
            ]),
            SourceDescription::default(),
            Some(Error::WrongType),
        ),
        (
            "bad count in header",
            Bytes::from_static(&[
                // v=2, p=0, count=1, SDES, len=12
                0x81, 0xca, 0x00, 0x0c,
            ]),
            SourceDescription::default(),
            Some(Error::InvalidHeader),
        ),
        (
            "empty string",
            Bytes::from_static(&[
                // v=2, p=0, count=1, SDES, len=12
                0x81, 0xca, 0x00, 0x0c, // ssrc=0x01020304
                0x01, 0x02, 0x03, 0x04, // CNAME, len=0
                0x01, 0x00, // END + padding
                0x00, 0x00,
            ]),
            SourceDescription {
                chunks: vec![SourceDescriptionChunk {
                    source: 0x01020304,
                    items: vec![SourceDescriptionItem {
                        sdes_type: SdesType::SdesCname,
                        text: Bytes::from_static(b""),
                    }],
                }],
            },
            None,
        ),
        (
            "two items",
            Bytes::from_static(&[
                // v=2, p=0, count=1, SDES, len=16
                0x81, 0xca, 0x00, 0x10, // ssrc=0x10000000
                0x10, 0x00, 0x00, 0x00, // CNAME, len=1, content=A
                0x01, 0x01, 0x41, // PHONE, len=1, content=B
                0x04, 0x01, 0x42, // END + padding
                0x00, 0x00,
            ]),
            SourceDescription {
                chunks: vec![SourceDescriptionChunk {
                    source: 0x10000000,
                    items: vec![
                        SourceDescriptionItem {
                            sdes_type: SdesType::SdesCname,
                            text: Bytes::from_static(b"A"),
                        },
                        SourceDescriptionItem {
                            sdes_type: SdesType::SdesPhone,
                            text: Bytes::from_static(b"B"),
                        },
                    ],
                }],
            },
            None,
        ),
        (
            "two chunks",
            Bytes::from_static(&[
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
            ]),
            SourceDescription {
                chunks: vec![
                    SourceDescriptionChunk {
                        source: 0x01020304,
                        items: vec![SourceDescriptionItem {
                            sdes_type: SdesType::SdesCname,
                            text: Bytes::from_static(b"A"),
                        }],
                    },
                    SourceDescriptionChunk {
                        source: 0x05060708,
                        items: vec![SourceDescriptionItem {
                            sdes_type: SdesType::SdesCname,
                            text: Bytes::from_static(b"BCD"),
                        }],
                    },
                ],
            },
            None,
        ),
    ];

    for (name, mut data, want, want_error) in tests {
        let got = SourceDescription::unmarshal(&mut data);

        assert_eq!(
            got.is_err(),
            want_error.is_some(),
            "Unmarshal {name}: err = {got:?}, want {want_error:?}"
        );

        if let Some(err) = want_error {
            let got_err = got.err().unwrap();
            assert_eq!(
                err, got_err,
                "Unmarshal {name}: err = {got_err:?}, want {err:?}",
            );
        } else {
            let actual = got.unwrap();
            assert_eq!(
                actual, want,
                "Unmarshal {name}: got {actual:?}, want {want:?}"
            );
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
                            sdes_type: SdesType::SdesCname,
                            text: Bytes::from_static(b"test@example.com"),
                        }],
                    },
                    SourceDescriptionChunk {
                        source: 2,
                        items: vec![
                            SourceDescriptionItem {
                                sdes_type: SdesType::SdesNote,
                                text: Bytes::from_static(b"some note"),
                            },
                            SourceDescriptionItem {
                                sdes_type: SdesType::SdesNote,
                                text: Bytes::from_static(b"another note"),
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
                        sdes_type: SdesType::SdesEnd,
                        text: Bytes::from_static(b"test@example.com"),
                    }],
                }],
            },
            Some(Error::SdesMissingType),
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
                        sdes_type: SdesType::SdesEmail,
                        text: Bytes::from_static(b"test@example.com"),
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
                        sdes_type: SdesType::SdesCname,
                        text: Bytes::from_static(b""),
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
                        sdes_type: SdesType::SdesCname,
                        text: Bytes::copy_from_slice(too_long_text.as_bytes()),
                    }],
                }],
            },
            Some(Error::SdesTextTooLong),
        ),
        (
            "count overflow",
            SourceDescription {
                chunks: too_many_chunks,
            },
            Some(Error::TooManyChunks),
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
            let actual = SourceDescription::unmarshal(&mut data)
                .unwrap_or_else(|_| panic!("Unmarshal {name}"));

            assert_eq!(
                actual, want,
                "{name} round trip: got {actual:?}, want {want:?}"
            )
        }
    }
}
