use super::*;

#[test]
fn test_sender_report_unmarshal() {
    let tests = vec![
        (
            "nil",
            Bytes::from_static(&[]),
            SenderReport::default(),
            Some(Error::PacketTooShort),
        ),
        (
            "valid",
            Bytes::from_static(&[
                0x81u8, 0xc8, 0x0, 0x7, // v=2, p=0, count=1, SR, len=7
                0x90, 0x2f, 0x9e, 0x2e, // ssrc=0x902f9e2e
                0xda, 0x8b, 0xd1, 0xfc, 0xdd, 0xdd, 0xa0, 0x5a, // ntp=0xda8bd1fcdddda05a
                0xaa, 0xf4, 0xed, 0xd5, // rtp=0xaaf4edd5
                0x00, 0x00, 0x00, 0x01, // packetCount=1
                0x00, 0x00, 0x00, 0x02, // octetCount=2
                0xbc, 0x5e, 0x9a, 0x40, // ssrc=0xbc5e9a40
                0x0, 0x0, 0x0, 0x0, // fracLost=0, totalLost=0
                0x0, 0x0, 0x46, 0xe1, // lastSeq=0x46e1
                0x0, 0x0, 0x1, 0x11, // jitter=273
                0x9, 0xf3, 0x64, 0x32, // lsr=0x9f36432
                0x0, 0x2, 0x4a, 0x79, // delay=150137
            ]),
            SenderReport {
                ssrc: 0x902f9e2e,
                ntp_time: 0xda8bd1fcdddda05a,
                rtp_time: 0xaaf4edd5,
                packet_count: 1,
                octet_count: 2,
                reports: vec![ReceptionReport {
                    ssrc: 0xbc5e9a40,
                    fraction_lost: 0,
                    total_lost: 0,
                    last_sequence_number: 0x46e1,
                    jitter: 273,
                    last_sender_report: 0x9f36432,
                    delay: 150137,
                }],
                profile_extensions: Bytes::from_static(&[]),
            },
            None,
        ),
        (
            "wrong type",
            Bytes::from_static(&[
                0x81, 0xc9, 0x0, 0x7, // v=2, p=0, count=1, RR, len=7
                0x90, 0x2f, 0x9e, 0x2e, // ssrc=0x902f9e2e
                0xda, 0x8b, 0xd1, 0xfc, 0xdd, 0xdd, 0xa0, 0x5a, // ntp=0xda8bd1fcdddda05a
                0xaa, 0xf4, 0xed, 0xd5, // rtp=0xaaf4edd5
                0x00, 0x00, 0x00, 0x01, // packetCount=1
                0x00, 0x00, 0x00, 0x02, // octetCount=2
                0xbc, 0x5e, 0x9a, 0x40, // ssrc=0xbc5e9a40
                0x0, 0x0, 0x0, 0x0, // fracLost=0, totalLost=0
                0x0, 0x0, 0x46, 0xe1, // jitter=273
                0x0, 0x0, 0x1, 0x11, // lastSeq=0x46e1
                0x9, 0xf3, 0x64, 0x32, // lsr=0x9f36432
                0x0, 0x2, 0x4a, 0x79, // delay=150137
            ]),
            SenderReport::default(),
            Some(Error::WrongType),
        ),
        (
            "bad count in header",
            Bytes::from_static(&[
                0x82, 0xc8, 0x0, 0x7, // v=2, p=0, count=1, SR, len=7
                0x90, 0x2f, 0x9e, 0x2e, // ssrc=0x902f9e2e
                0xda, 0x8b, 0xd1, 0xfc, 0xdd, 0xdd, 0xa0, 0x5a, // ntp=0xda8bd1fcdddda05a
                0xaa, 0xf4, 0xed, 0xd5, // rtp=0xaaf4edd5
                0x00, 0x00, 0x00, 0x01, // packetCount=1
                0x00, 0x00, 0x00, 0x02, // octetCount=2
                0xbc, 0x5e, 0x9a, 0x40, // ssrc=0xbc5e9a40
                0x0, 0x0, 0x0, 0x0, // fracLost=0, totalLost=0
                0x0, 0x0, 0x46, 0xe1, // lastSeq=0x46e1
                0x0, 0x0, 0x1, 0x11, // jitter=273
                0x9, 0xf3, 0x64, 0x32, // lsr=0x9f36432
                0x0, 0x2, 0x4a, 0x79, // delay=150137
            ]),
            SenderReport::default(),
            Some(Error::PacketTooShort),
        ),
        (
            "with extension", // issue #447
            Bytes::from_static(&[
                0x80, 0xc8, 0x0, 0x6, // v=2, p=0, count=0, SR, len=6
                0x2b, 0x7e, 0xc0, 0xc5, // ssrc=0x2b7ec0c5
                0xe0, 0x20, 0xa2, 0xa9, 0x52, 0xa5, 0x3f, 0xc0, // ntp=0xe020a2a952a53fc0
                0x2e, 0x48, 0xa5, 0x52, // rtp=0x2e48a552
                0x0, 0x0, 0x0, 0x46, // packetCount=70
                0x0, 0x0, 0x12, 0x1d, // octetCount=4637
                0x81, 0xca, 0x0, 0x6, 0x2b, 0x7e, 0xc0, 0xc5, 0x1, 0x10, 0x4c, 0x63, 0x49, 0x66,
                0x7a, 0x58, 0x6f, 0x6e, 0x44, 0x6f, 0x72, 0x64, 0x53, 0x65, 0x57, 0x36, 0x0,
                0x0, // profile-specific extension
            ]),
            SenderReport {
                ssrc: 0x2b7ec0c5,
                ntp_time: 0xe020a2a952a53fc0,
                rtp_time: 0x2e48a552,
                packet_count: 70,
                octet_count: 4637,
                reports: vec![],
                profile_extensions: Bytes::from_static(&[
                    0x81, 0xca, 0x0, 0x6, 0x2b, 0x7e, 0xc0, 0xc5, 0x1, 0x10, 0x4c, 0x63, 0x49,
                    0x66, 0x7a, 0x58, 0x6f, 0x6e, 0x44, 0x6f, 0x72, 0x64, 0x53, 0x65, 0x57, 0x36,
                    0x0, 0x0,
                ]),
            },
            None,
        ),
    ];

    for (name, mut data, want, want_error) in tests {
        let got = SenderReport::unmarshal(&mut data);

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
fn test_sender_report_roundtrip() {
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
            SenderReport {
                ssrc: 1,
                ntp_time: 999,
                rtp_time: 555,
                packet_count: 32,
                octet_count: 11,
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
            SenderReport {
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
            "extension",
            SenderReport {
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
                profile_extensions: Bytes::from_static(&[1, 2, 3, 4]),
                ..Default::default()
            },
            None,
        ),
        (
            "count overflow",
            SenderReport {
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
                SenderReport::unmarshal(&mut data).unwrap_or_else(|_| panic!("Unmarshal {name}"));

            assert_eq!(
                actual, want,
                "{name} round trip: got {actual:?}, want {want:?}"
            )
        }
    }
}
