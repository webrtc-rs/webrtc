use super::*;

use std::io::{BufReader, BufWriter};

use util::Error;

#[test]
fn test_receiver_report_unmarshal() -> Result<(), Error> {
    let tests = vec![
        (
            "valid",
            vec![
                // v=2, p=0, count=1, RR, len=7
                0x81, 0xc9, 0x0, 0x7, // ssrc=0x902f9e2e
                0x90, 0x2f, 0x9e, 0x2e, // ssrc=0xbc5e9a40
                0xbc, 0x5e, 0x9a, 0x40, // fracLost=0, totalLost=0
                0x0, 0x0, 0x0, 0x0, // lastSeq=0x46e1
                0x0, 0x0, 0x46, 0xe1, // jitter=273
                0x0, 0x0, 0x1, 0x11, // lsr=0x9f36432
                0x9, 0xf3, 0x64, 0x32, // delay=150137
                0x0, 0x2, 0x4a, 0x79,
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
            None,
        ),
        (
            "valid with extension data",
            vec![
                // v=2, p=0, count=1, RR, len=9
                0x81, 0xc9, 0x0, 0x9, // ssrc=0x902f9e2e
                0x90, 0x2f, 0x9e, 0x2e, // ssrc=0xbc5e9a40
                0xbc, 0x5e, 0x9a, 0x40, // fracLost=0, totalLost=0
                0x0, 0x0, 0x0, 0x0, // lastSeq=0x46e1
                0x0, 0x0, 0x46, 0xe1, // jitter=273
                0x0, 0x0, 0x1, 0x11, // lsr=0x9f36432
                0x9, 0xf3, 0x64, 0x32, // delay=150137
                0x0, 0x2, 0x4a, 0x79, // profile-specific extension data
                0x54, 0x45, 0x53, 0x54, 0x44, 0x41, 0x54, 0x41,
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
            None,
        ),
        (
            "short report",
            vec![
                // v=2, p=0, count=1, RR, len=7
                0x81, 0xc9, 0x00, 0x0c, // ssrc=0x902f9e2e
                0x90, 0x2f, 0x9e, 0x2e, // fracLost=0, totalLost=0
                0x00, 0x00, 0x00, 0x00,
                // report ends early
            ],
            ReceiverReport::default(),
            Some(ERR_FAILED_TO_FILL_WHOLE_BUFFER.clone()),
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
            Some(ERR_WRONG_TYPE.clone()),
        ),
        (
            "bad count in header",
            vec![
                // v=2, p=0, count=2, RR, len=7
                0x82, 0xc9, 0x0, 0x7, // ssrc=0x902f9e2e
                0x90, 0x2f, 0x9e, 0x2e, // ssrc=0xbc5e9a40
                0xbc, 0x5e, 0x9a, 0x40, // fracLost=0, totalLost=0
                0x0, 0x0, 0x0, 0x0, // lastSeq=0x46e1
                0x0, 0x0, 0x46, 0xe1, // jitter=273
                0x0, 0x0, 0x1, 0x11, // lsr=0x9f36432
                0x9, 0xf3, 0x64, 0x32, // delay=150137
                0x0, 0x2, 0x4a, 0x79,
            ],
            ReceiverReport::default(),
            Some(ERR_FAILED_TO_FILL_WHOLE_BUFFER.clone()),
        ),
        (
            "nil",
            vec![],
            ReceiverReport::default(),
            Some(ERR_FAILED_TO_FILL_WHOLE_BUFFER.clone()),
        ),
    ];

    for (name, data, want, want_error) in tests {
        let mut reader = BufReader::new(data.as_slice());
        let result = ReceiverReport::unmarshal(&mut reader);
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
fn test_receiver_report_roundtrip() -> Result<(), Error> {
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
            Some(ERR_INVALID_TOTAL_LOST.clone()),
        ),
        (
            "count overflow",
            ReceiverReport {
                ssrc: 1,
                reports: too_many_reports,
                ..Default::default()
            },
            Some(ERR_TOO_MANY_REPORTS.clone()),
        ),
    ];

    for (name, report, marshal_error) in tests {
        let mut data: Vec<u8> = vec![];
        {
            let mut writer = BufWriter::<&mut Vec<u8>>::new(data.as_mut());
            let result = report.marshal(&mut writer);
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
        let decoded = ReceiverReport::unmarshal(&mut reader)?;
        assert_eq!(
            decoded, report,
            "{} header round trip: got {:?}, want {:?}",
            name, decoded, report
        )
    }

    Ok(())
}
