#[cfg(test)]
mod test {
    use crate::{
        error::Error, packet::Packet, receiver_report::*, reception_report::ReceptionReport,
    };

    #[test]
    fn test_receiver_report_unmarshal() {
        let tests = vec![
            (
                "valid",
                vec![
                    0x81u8, 0xc9, 0x0, 0x7, // v=2, p=0, count=1, RR, len=7
                    0x90, 0x2f, 0x9e, 0x2e, // ssrc=0x902f9e2e
                    0xbc, 0x5e, 0x9a, 0x40, // ssrc=0xbc5e9a40
                    0x0, 0x0, 0x0, 0x0, // fracLost=0, totalLost=0
                    0x0, 0x0, 0x46, 0xe1, // lastSeq=0x46e1
                    0x0, 0x0, 0x1, 0x11, // jitter=273
                    0x9, 0xf3, 0x64, 0x32, // lsr=0x9f36432
                    0x0, 0x2, 0x4a, 0x79, // delay=150137
                ],
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
                    profile_extensions: vec![],
                },
                Ok(()),
            ),
            (
                "valid with extension data",
                vec![
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
                ],
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
                    profile_extensions: vec![0x54, 0x45, 0x53, 0x54, 0x44, 0x41, 0x54, 0x41],
                },
                Ok(()),
            ),
            (
                "short report",
                vec![
                    0x81, 0xc9, 0x00, 0x0c, // v=2, p=0, count=1, RR, len=7
                    0x90, 0x2f, 0x9e, 0x2e, // ssrc=0x902f9e2e
                    0x00, 0x00, 0x00,
                    0x00, // fracLost=0, totalLost=0
                          // report ends early
                ],
                ReceiverReport::default(),
                Err(Error::PacketTooShort),
            ),
            (
                "wrong type",
                vec![
                    // v=2, p=0, count=1, SR, len=7
                    0x81, 0xc8, 0x0, 0x7, // ssrc=0x902f9e2e
                    0x90, 0x2f, 0x9e, 0x2e, // ssrc=0xbc5e9a40
                    0xbc, 0x5e, 0x9a, 0x40, // fracLost=0, totalLost=0
                    0x0, 0x0, 0x0, 0x0, // lastSeq=0x46e1
                    0x0, 0x0, 0x46, 0xe1, // jitter=273
                    0x0, 0x0, 0x1, 0x11, // lsr=0x9f36432
                    0x9, 0xf3, 0x64, 0x32, // delay=150137
                    0x0, 0x2, 0x4a, 0x79,
                ],
                ReceiverReport::default(),
                Err(Error::WrongType),
            ),
            (
                "bad count in header",
                vec![
                    0x82, 0xc9, 0x0, 0x7, // v=2, p=0, count=2, RR, len=7
                    0x90, 0x2f, 0x9e, 0x2e, // ssrc=0x902f9e2e
                    0xbc, 0x5e, 0x9a, 0x40, // ssrc=0xbc5e9a40
                    0x0, 0x0, 0x0, 0x0, // fracLost=0, totalLost=0
                    0x0, 0x0, 0x46, 0xe1, // lastSeq=0x46e1
                    0x0, 0x0, 0x1, 0x11, // jitter=273
                    0x9, 0xf3, 0x64, 0x32, // lsr=0x9f36432
                    0x0, 0x2, 0x4a, 0x79, // delay=150137
                ],
                ReceiverReport::default(),
                Err(Error::InvalidHeader),
            ),
            (
                "nil",
                vec![],
                ReceiverReport::default(),
                Err(Error::PacketTooShort),
            ),
        ];

        for (name, data, want, want_error) in tests {
            let mut rr = ReceiverReport::default();
            let result = rr.unmarshal(&mut data[..].into());

            assert_eq!(
                result.clone().err(),
                want_error.clone().err(),
                "Unmarshal {} rr: err = {:?}, want {:?}",
                name,
                result,
                want_error
            );

            match result {
                Ok(_) => {
                    assert_eq!(
                        rr, want,
                        "Unmarshal {} rr: got {:?}, want {:?}",
                        name, rr, want
                    )
                }

                Err(_) => continue,
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
                    profile_extensions: vec![],
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

        for (name, report, marshal_error) in tests {
            let data = report.marshal();

            assert_eq!(
                data.clone().err(),
                marshal_error,
                "Marshal {}: err = {:?}, want {:?}",
                name,
                data.err(),
                marshal_error
            );

            match data {
                Ok(mut e) => {
                    let mut decoded = ReceiverReport::default();

                    decoded
                        .unmarshal(&mut e)
                        .expect(format!("Unmarsahll {}", name).as_str());

                    assert_eq!(
                        decoded, report,
                        "{} rr round trip: got {:?}, want {:?}",
                        name, decoded, report
                    )
                }

                Err(_) => continue,
            }
        }
    }
}
