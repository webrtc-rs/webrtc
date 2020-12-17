#[cfg(test)]
mod test {
    use std::vec;

    use crate::goodbye::*;

    #[test]
    fn test_goodbye_unmarshal() {
        let tests = vec![
            (
                "valid",
                vec![
                    // v=2, p=0, count=1, BYE, len=12
                    0x81, 0xcb, 0x00, 0x0c, // ssrc=0x902f9e2e
                    0x90, 0x2f, 0x9e, 0x2e, // len=3, text=FOO
                    0x03, 0x46, 0x4f, 0x4f,
                ],
                Goodbye {
                    sources: vec![0x902f9e2e],
                    reason: "FOO".to_string(),
                },
                Ok(()),
            ),
            (
                "invalid octet count",
                vec![
                    // v=2, p=0, count=1, BYE, len=12
                    0x81, 0xcb, 0x00, 0x0c, // ssrc=0x902f9e2e
                    0x90, 0x2f, 0x9e, 0x2e, // len=4, text=FOO
                    0x04, 0x46, 0x4f, 0x4f,
                ],
                Goodbye {
                    sources: vec![],
                    reason: "".to_string(),
                },
                Err(ERR_PACKET_TOO_SHORT.clone()),
            ),
            (
                "wrong type",
                vec![
                    // v=2, p=0, count=1, SDES, len=12
                    0x81, 0xca, 0x00, 0x0c, // ssrc=0x902f9e2e
                    0x90, 0x2f, 0x9e, 0x2e, // len=3, text=FOO
                    0x03, 0x46, 0x4f, 0x4f,
                ],
                Goodbye {
                    sources: vec![],
                    reason: "".to_string(),
                },
                Err(ERR_WRONG_TYPE.clone()),
            ),
            (
                "short reason",
                vec![
                    // v=2, p=0, count=1, BYE, len=12
                    0x81, 0xcb, 0x00, 0x0c, // ssrc=0x902f9e2e
                    0x90, 0x2f, 0x9e, 0x2e, // len=3, text=F + padding
                    0x01, 0x46, 0x00, 0x00,
                ],
                Goodbye {
                    sources: vec![0x902f9e2e],
                    reason: "F".to_string(),
                },
                Ok(()),
            ),
            (
                "not byte aligned",
                vec![
                    // v=2, p=0, count=1, BYE, len=10
                    0x81, 0xcb, 0x00, 0x0a, // ssrc=0x902f9e2e
                    0x90, 0x2f, 0x9e, 0x2e, // len=1, text=F
                    0x01, 0x46,
                ],
                Goodbye {
                    sources: vec![],
                    reason: "".to_string(),
                },
                Err(ERR_PACKET_TOO_SHORT.clone()),
            ),
            (
                "bad count in header",
                vec![
                    // v=2, p=0, count=2, BYE, len=8
                    0x82, 0xcb, 0x00, 0x0c, // ssrc=0x902f9e2e
                    0x90, 0x2f, 0x9e, 0x2e,
                ],
                Goodbye {
                    sources: vec![],
                    reason: "".to_string(),
                },
                Err(ERR_PACKET_TOO_SHORT.clone()),
            ),
            (
                "empty packet",
                vec![
                    // v=2, p=0, count=0, BYE, len=4
                    0x80, 0xcb, 0x00, 0x04,
                ],
                Goodbye {
                    sources: vec![],
                    reason: "".to_string(),
                },
                Ok(()),
            ),
            (
                "nil",
                vec![],
                Goodbye {
                    sources: vec![],
                    reason: "".to_string(),
                },
                Err(ERR_PACKET_TOO_SHORT.clone()),
            ),
        ];

        for (name, data, want, want_error) in tests {
            let mut bye = Goodbye::default();

            let got = bye.unmarshal(&mut data.as_slice().into());

            assert_eq!(
                got, want_error,
                "Unmarshal {} bye: err = {:?}, want {:?}",
                name, got, want_error
            );

            match got {
                Ok(_) => {
                    assert_eq!(
                        bye, want,
                        "Unmarshal {} bye: got {:?}, want {:?}",
                        name, bye, want
                    )
                }

                Err(_) => continue,
            }
        }
    }

    #[test]
    fn test_goodbye_round_trip() {
        let mut too_many_sources = vec![0u32; 1 << 5];

        let mut too_long_text = String::new();
        for _ in 0..1 << 8 {
            too_long_text.push('x');
        }

        let tests = vec![
            (
                "empty",
                Goodbye {
                    sources: vec![],
                    ..Default::default()
                },
                Ok(()),
            ),
            (
                "valid",
                Goodbye {
                    sources: vec![0x01020304, 0x05060708],
                    reason: "because".to_owned(),
                },
                Ok(()),
            ),
            (
                "empty reason",
                Goodbye {
                    sources: vec![0x01020304],
                    reason: "".to_owned(),
                },
                Ok(()),
            ),
            (
                "reason no source",
                Goodbye {
                    sources: vec![],
                    reason: "foo".to_owned(),
                },
                Ok(()),
            ),
            (
                "short reason",
                Goodbye {
                    sources: vec![],
                    reason: "f".to_owned(),
                },
                Ok(()),
            ),
            (
                "count overflow",
                Goodbye {
                    sources: too_many_sources.clone(),
                    reason: "".to_owned(),
                },
                Err(ERR_TOO_MANY_SOURCES.to_owned()),
            ),
            (
                "reason too long",
                Goodbye {
                    sources: vec![],
                    reason: too_long_text,
                },
                Err(ERR_REASON_TOO_LONG.to_owned()),
            ),
        ];

        for (name, want_bye, want_error) in tests {
            let want = want_bye.marshal();

            assert_eq!(
                want.is_ok(),
                want_error.is_ok(),
                "Marshal {}: err = {:?}, want {:?}",
                name,
                want,
                want_error
            );

            match want {
                Ok(mut e) => {
                    let mut bye = Goodbye::default();
                    bye.unmarshal(&mut e)
                        .expect(format!("Unmarshal {}", name).as_str());

                    assert_eq!(
                        bye, want_bye,
                        "{} sdes round trip: got {:?}, want {:?}",
                        name, bye, want_bye
                    )
                }

                Err(_) => continue,
            }
        }
    }
}
