use super::*;

use std::io::{BufReader, BufWriter};

use util::Error;

#[test]
fn test_sender_report_unmarshal() -> Result<(), Error> {
    let tests = vec![
        (
            "nil",
            vec![],
            SenderReport::default(),
            Some(ERR_FAILED_TO_FILL_WHOLE_BUFFER.clone()),
        ),
        (
            "valid",
            vec![
                // v=2, p=0, count=1, SR, len=7
                0x81, 0xc8, 0x0, 0x7, // ssrc=0x902f9e2e
                0x90, 0x2f, 0x9e, 0x2e, // ntp=0xda8bd1fcdddda05a
                0xda, 0x8b, 0xd1, 0xfc, 0xdd, 0xdd, 0xa0, 0x5a, // rtp=0xaaf4edd5
                0xaa, 0xf4, 0xed, 0xd5, // packetCount=1
                0x00, 0x00, 0x00, 0x01, // octetCount=2
                0x00, 0x00, 0x00, 0x02, // ssrc=0xbc5e9a40
                0xbc, 0x5e, 0x9a, 0x40, // fracLost=0, totalLost=0
                0x0, 0x0, 0x0, 0x0, // lastSeq=0x46e1
                0x0, 0x0, 0x46, 0xe1, // jitter=273
                0x0, 0x0, 0x1, 0x11, // lsr=0x9f36432
                0x9, 0xf3, 0x64, 0x32, // delay=150137
                0x0, 0x2, 0x4a, 0x79,
            ],
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
                profile_extensions: vec![],
            },
            None,
        ),
        (
            "wrong type",
            vec![
                // v=2, p=0, count=1, RR, len=7
                0x81, 0xc9, 0x0, 0x7, // ssrc=0x902f9e2e
                0x90, 0x2f, 0x9e, 0x2e, // ntp=0xda8bd1fcdddda05a
                0xda, 0x8b, 0xd1, 0xfc, 0xdd, 0xdd, 0xa0, 0x5a, // rtp=0xaaf4edd5
                0xaa, 0xf4, 0xed, 0xd5, // packetCount=1
                0x00, 0x00, 0x00, 0x01, // octetCount=2
                0x00, 0x00, 0x00, 0x02, // ssrc=0xbc5e9a40
                0xbc, 0x5e, 0x9a, 0x40, // fracLost=0, totalLost=0
                0x0, 0x0, 0x0, 0x0, // lastSeq=0x46e1
                0x0, 0x0, 0x46, 0xe1, // jitter=273
                0x0, 0x0, 0x1, 0x11, // lsr=0x9f36432
                0x9, 0xf3, 0x64, 0x32, // delay=150137
                0x0, 0x2, 0x4a, 0x79,
            ],
            SenderReport::default(),
            Some(ERR_WRONG_TYPE.clone()),
        ),
        (
            "bad count in header",
            vec![
                // v=2, p=0, count=1, SR, len=7
                0x82, 0xc8, 0x0, 0x7, // ssrc=0x902f9e2e
                0x90, 0x2f, 0x9e, 0x2e, // ntp=0xda8bd1fcdddda05a
                0xda, 0x8b, 0xd1, 0xfc, 0xdd, 0xdd, 0xa0, 0x5a, // rtp=0xaaf4edd5
                0xaa, 0xf4, 0xed, 0xd5, // packetCount=1
                0x00, 0x00, 0x00, 0x01, // octetCount=2
                0x00, 0x00, 0x00, 0x02, // ssrc=0xbc5e9a40
                0xbc, 0x5e, 0x9a, 0x40, // fracLost=0, totalLost=0
                0x0, 0x0, 0x0, 0x0, // lastSeq=0x46e1
                0x0, 0x0, 0x46, 0xe1, // jitter=273
                0x0, 0x0, 0x1, 0x11, // lsr=0x9f36432
                0x9, 0xf3, 0x64, 0x32, // delay=150137
                0x0, 0x2, 0x4a, 0x79,
            ],
            SenderReport::default(),
            Some(ERR_FAILED_TO_FILL_WHOLE_BUFFER.clone()),
        ),
        (
            "with extension", // issue #447
            vec![
                // v=2, p=0, count=0, SR, len=6
                0x80, 0xc8, 0x0, 0x6, // ssrc=0x2b7ec0c5
                0x2b, 0x7e, 0xc0, 0xc5, // ntp=0xe020a2a952a53fc0
                0xe0, 0x20, 0xa2, 0xa9, 0x52, 0xa5, 0x3f, 0xc0, // rtp=0x2e48a552
                0x2e, 0x48, 0xa5, 0x52, // packetCount=70
                0x0, 0x0, 0x0, 0x46, // octetCount=4637
                0x0, 0x0, 0x12, 0x1d, // profile-specific extension
                0x81, 0xca, 0x0, 0x6, 0x2b, 0x7e, 0xc0, 0xc5, 0x1, 0x10, 0x4c, 0x63, 0x49, 0x66,
                0x7a, 0x58, 0x6f, 0x6e, 0x44, 0x6f, 0x72, 0x64, 0x53, 0x65, 0x57, 0x36, 0x0, 0x0,
            ],
            SenderReport {
                ssrc: 0x2b7ec0c5,
                ntp_time: 0xe020a2a952a53fc0,
                rtp_time: 0x2e48a552,
                packet_count: 70,
                octet_count: 4637,
                reports: vec![],
                profile_extensions: vec![
                    0x81, 0xca, 0x0, 0x6, 0x2b, 0x7e, 0xc0, 0xc5, 0x1, 0x10, 0x4c, 0x63, 0x49,
                    0x66, 0x7a, 0x58, 0x6f, 0x6e, 0x44, 0x6f, 0x72, 0x64, 0x53, 0x65, 0x57, 0x36,
                    0x0, 0x0,
                ],
            },
            None,
        ),
    ];

    for (name, data, want, want_error) in tests {
        let mut reader = BufReader::new(data.as_slice());
        let result = SenderReport::unmarshal(&mut reader);
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
                );

                let mut ssrc_found = false;
                let dst_ssrc = got.destination_ssrc();
                for v in &dst_ssrc {
                    if *v == got.ssrc {
                        ssrc_found = true;
                        break;
                    }
                }
                assert_eq!(
                    ssrc_found, true,
                    "Unmarshal {} sr: sr's destination_ssrc should include it's SSRC field",
                    name
                );
            } else {
                assert!(false, "must no error in test {}", name);
            }
        }
    }

    Ok(())
}

#[test]
fn test_sender_report_roundtrip() -> Result<(), Error> {
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
                profile_extensions: vec![],
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
                profile_extensions: vec![1, 2, 3, 4],
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
        let decoded = SenderReport::unmarshal(&mut reader)?;
        assert_eq!(
            decoded, report,
            "{} header round trip: got {:?}, want {:?}",
            name, decoded, report
        )
    }

    Ok(())
}
