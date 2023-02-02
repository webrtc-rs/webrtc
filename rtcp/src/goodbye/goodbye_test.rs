use super::*;

#[test]
fn test_goodbye_unmarshal() {
    let tests = vec![
        (
            "valid",
            Bytes::from_static(&[
                0x81, 0xcb, 0x00, 0x0c, // v=2, p=0, count=1, BYE, len=12
                0x90, 0x2f, 0x9e, 0x2e, // ssrc=0x902f9e2e
                0x03, 0x46, 0x4f, 0x4f, // len=3, text=FOO
            ]),
            Goodbye {
                sources: vec![0x902f9e2e],
                reason: Bytes::from_static(b"FOO"),
            },
            None,
        ),
        (
            "invalid octet count",
            Bytes::from_static(&[
                0x81, 0xcb, 0x00, 0x0c, // v=2, p=0, count=1, BYE, len=12
                0x90, 0x2f, 0x9e, 0x2e, // ssrc=0x902f9e2e
                0x04, 0x46, 0x4f, 0x4f, // len=4, text=FOO
            ]),
            Goodbye {
                sources: vec![],
                reason: Bytes::from_static(b""),
            },
            Some(Error::PacketTooShort),
        ),
        (
            "wrong type",
            Bytes::from_static(&[
                0x81, 0xca, 0x00, 0x0c, // v=2, p=0, count=1, SDES, len=12
                0x90, 0x2f, 0x9e, 0x2e, // ssrc=0x902f9e2e
                0x03, 0x46, 0x4f, 0x4f, // len=3, text=FOO
            ]),
            Goodbye {
                sources: vec![],
                reason: Bytes::from_static(b""),
            },
            Some(Error::WrongType),
        ),
        (
            "short reason",
            Bytes::from_static(&[
                0x81, 0xcb, 0x00, 0x0c, // v=2, p=0, count=1, BYE, len=12
                0x90, 0x2f, 0x9e, 0x2e, // ssrc=0x902f9e2e
                0x01, 0x46, 0x00, 0x00, // len=3, text=F + padding
            ]),
            Goodbye {
                sources: vec![0x902f9e2e],
                reason: Bytes::from_static(b"F"),
            },
            None,
        ),
        (
            "not byte aligned",
            Bytes::from_static(&[
                0x81, 0xcb, 0x00, 0x0a, // v=2, p=0, count=1, BYE, len=10
                0x90, 0x2f, 0x9e, 0x2e, // ssrc=0x902f9e2e
                0x01, 0x46, // len=1, text=F
            ]),
            Goodbye {
                sources: vec![],
                reason: Bytes::from_static(b""),
            },
            Some(Error::PacketTooShort),
        ),
        (
            "bad count in header",
            Bytes::from_static(&[
                0x82, 0xcb, 0x00, 0x0c, // v=2, p=0, count=2, BYE, len=8
                0x90, 0x2f, 0x9e, 0x2e, // ssrc=0x902f9e2e
            ]),
            Goodbye {
                sources: vec![],
                reason: Bytes::from_static(b""),
            },
            Some(Error::PacketTooShort),
        ),
        (
            "empty packet",
            Bytes::from_static(&[
                // v=2, p=0, count=0, BYE, len=4
                0x80, 0xcb, 0x00, 0x04,
            ]),
            Goodbye {
                sources: vec![],
                reason: Bytes::from_static(b""),
            },
            None,
        ),
        (
            "nil",
            Bytes::from_static(&[]),
            Goodbye {
                sources: vec![],
                reason: Bytes::from_static(b""),
            },
            Some(Error::PacketTooShort),
        ),
    ];

    for (name, mut data, want, want_error) in tests {
        let got = Goodbye::unmarshal(&mut data);

        assert_eq!(
            got.is_err(),
            want_error.is_some(),
            "Unmarshal {name} bye: err = {got:?}, want {want_error:?}"
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
fn test_goodbye_round_trip() {
    let too_many_sources = vec![0u32; 1 << 5];

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
            None,
        ),
        (
            "valid",
            Goodbye {
                sources: vec![0x01020304, 0x05060708],
                reason: Bytes::from_static(b"because"),
            },
            None,
        ),
        (
            "empty reason",
            Goodbye {
                sources: vec![0x01020304],
                reason: Bytes::from_static(b""),
            },
            None,
        ),
        (
            "reason no source",
            Goodbye {
                sources: vec![],
                reason: Bytes::from_static(b"foo"),
            },
            None,
        ),
        (
            "short reason",
            Goodbye {
                sources: vec![],
                reason: Bytes::from_static(b"f"),
            },
            None,
        ),
        (
            "count overflow",
            Goodbye {
                sources: too_many_sources,
                reason: Bytes::from_static(b""),
            },
            Some(Error::TooManySources),
        ),
        (
            "reason too long",
            Goodbye {
                sources: vec![],
                reason: Bytes::copy_from_slice(too_long_text.as_bytes()),
            },
            Some(Error::ReasonTooLong),
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
            let actual =
                Goodbye::unmarshal(&mut data).unwrap_or_else(|_| panic!("Unmarshal {name}"));

            assert_eq!(
                actual, want,
                "{name} round trip: got {actual:?}, want {want:?}"
            )
        }
    }
}
