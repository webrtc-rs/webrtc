use super::*;

#[test]
fn test_receiver_report_unmarshal() {
    let tests = vec![
        (
            "valid",
            Bytes::from_static(&[
                0x81u8, 0xc9, 0x0, 0x7, // v=2, p=0, count=1, RR, len=7
                0x90, 0x2f, 0x9e, 0x2e, // ssrc=0x902f9e2e
                0xbc, 0x5e, 0x9a, 0x40, // ssrc=0xbc5e9a40
                0x0, 0x0, 0x0, 0x0, // fracLost=0, totalLost=0
                0x0, 0x0, 0x46, 0xe1, // lastSeq=0x46e1
                0x0, 0x0, 0x1, 0x11, // jitter=273
                0x9, 0xf3, 0x64, 0x32, // lsr=0x9f36432
                0x0, 0x2, 0x4a, 0x79, // delay=150137
            ]),
            ReceiverReport {
                ssrc: 0x902f9e2e,
                reports: vec![ReceptionReport {
                    ssrc: 0xbc5e9a40,
                    fraction_lost: 0,
                    total_lost: 0,
                    last_sequence_number: 0x46e1,
                    jitter: 273,
                    last_sender_report: 0x9f36432,
                    delay: 150137,
                }],
                profile_extensions: Bytes::new(),
            },
            None,
        ),
        (
            "valid with extension data",
            Bytes::from_static(&[
                0x81, 0xc9, 0x0, 0x9, // v=2, p=0, count=1, RR, len=9
                0x90, 0x2f, 0x9e, 0x2e, // ssrc=0x902f9e2e
                0xbc, 0x5e, 0x9a, 0x40, // ssrc=0xbc5e9a40
                0x0, 0x0, 0x0, 0x0, // fracLost=0, totalLost=0
                0x0, 0x0, 0x46, 0xe1, // lastSeq=0x46e1
                0x0, 0x0, 0x1, 0x11, // jitter=273
                0x9, 0xf3, 0x64, 0x32, // lsr=0x9f36432
                0x0, 0x2, 0x4a, 0x79, // delay=150137
                0x54, 0x45, 0x53, 0x54, 0x44, 0x41, 0x54,
                0x41, // profile-specific extension data
            ]),
            ReceiverReport {
                ssrc: 0x902f9e2e,
                reports: vec![ReceptionReport {
                    ssrc: 0xbc5e9a40,
                    fraction_lost: 0,
                    total_lost: 0,
                    last_sequence_number: 0x46e1,
                    jitter: 273,
                    last_sender_report: 0x9f36432,
                    delay: 150137,
                }],
                profile_extensions: Bytes::from_static(&[
                    0x54, 0x45, 0x53, 0x54, 0x44, 0x41, 0x54, 0x41,
                ]),
            },
            None,
        ),
        (
            "short report",
            Bytes::from_static(&[
                0x81, 0xc9, 0x00, 0x0c, // v=2, p=0, count=1, RR, len=7
                0x90, 0x2f, 0x9e, 0x2e, // ssrc=0x902f9e2e
                0x00, 0x00, 0x00,
                0x00, // fracLost=0, totalLost=0
                      // report ends early
            ]),
            ReceiverReport::default(),
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
            ReceiverReport::default(),
            Some(Error::WrongType),
        ),
        (
            "bad count in header",
            Bytes::from_static(&[
                0x82, 0xc9, 0x0, 0x7, // v=2, p=0, count=2, RR, len=7
                0x90, 0x2f, 0x9e, 0x2e, // ssrc=0x902f9e2e
                0xbc, 0x5e, 0x9a, 0x40, // ssrc=0xbc5e9a40
                0x0, 0x0, 0x0, 0x0, // fracLost=0, totalLost=0
                0x0, 0x0, 0x46, 0xe1, // lastSeq=0x46e1
                0x0, 0x0, 0x1, 0x11, // jitter=273
                0x9, 0xf3, 0x64, 0x32, // lsr=0x9f36432
                0x0, 0x2, 0x4a, 0x79, // delay=150137
            ]),
            ReceiverReport::default(),
            Some(Error::PacketTooShort),
        ),
        (
            "nil",
            Bytes::from_static(&[]),
            ReceiverReport::default(),
            Some(Error::PacketTooShort),
        ),
    ];

    for (name, mut data, want, want_error) in tests {
        let got = ReceiverReport::unmarshal(&mut data);

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
fn test_receiver_report_roundtrip() {
    let mut too_many_reports = vec![];
    for _i in 0..(1 << 5) {
        too_many_reports.push(ReceptionReport {
            ssrc: 2,
            fraction_lost: 2,
            total_lost: 3,
            last_sequence_number: 4,
            jitter: 5,
            last_sender_report: 6,
            delay: 7,
        });
    }

    let tests = vec![
        (
            "valid",
            ReceiverReport {
                ssrc: 1,
                reports: vec![
                    ReceptionReport {
                        ssrc: 2,
                        fraction_lost: 2,
                        total_lost: 3,
                        last_sequence_number: 4,
                        jitter: 5,
                        last_sender_report: 6,
                        delay: 7,
                    },
                    ReceptionReport::default(),
                ],
                profile_extensions: Bytes::from_static(&[]),
            },
            None,
        ),
        (
            "also valid",
            ReceiverReport {
                ssrc: 2,
                reports: vec![ReceptionReport {
                    ssrc: 999,
                    fraction_lost: 30,
                    total_lost: 12345,
                    last_sequence_number: 99,
                    jitter: 22,
                    last_sender_report: 92,
                    delay: 46,
                }],
                ..Default::default()
            },
            None,
        ),
        (
            "totallost overflow",
            ReceiverReport {
                ssrc: 1,
                reports: vec![ReceptionReport {
                    total_lost: 1 << 25,
                    ..Default::default()
                }],
                ..Default::default()
            },
            Some(Error::InvalidTotalLost),
        ),
        (
            "count overflow",
            ReceiverReport {
                ssrc: 1,
                reports: too_many_reports,
                ..Default::default()
            },
            Some(Error::TooManyReports),
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
                ReceiverReport::unmarshal(&mut data).unwrap_or_else(|_| panic!("Unmarshal {name}"));

            assert_eq!(
                actual, want,
                "{name} round trip: got {actual:?}, want {want:?}"
            )
        }
    }
}
