use super::*;

use std::io::{BufReader, BufWriter};

use util::Error;

#[test]
fn test_receiver_estimated_maximum_bitrate_unmarshal() -> Result<(), Error> {
    let tests = vec![
        (
            "valid",
            vec![
                143, 206, 0, 5, 0, 0, 0, 1, 0, 0, 0, 0, 82, 69, 77, 66, 1, 26, 32, 223, 72, 116,
                237, 22,
            ],
            ReceiverEstimatedMaximumBitrate {
                sender_ssrc: 1,
                bitrate: 8927168,
                ssrcs: vec![1215622422],
            },
            None,
        ),
        (
            "Real data sent by Chrome while watching a 6Mb/s stream",
            // mantissa = []byte{26 & 3, 32, 223} = []byte{2, 32, 223} = 139487
            // exp = 26 >> 2 = 6
            // bitrate = 139487 * 2^6 = 139487 * 64 = 8927168 = 8.9 Mb/s
            vec![
                143, 206, 0, 5, 0, 0, 0, 1, 0, 0, 0, 0, 82, 69, 77, 66, 1, 26, 32, 223, 72, 116,
                237, 22,
            ],
            ReceiverEstimatedMaximumBitrate {
                sender_ssrc: 1,
                bitrate: 8927168,
                ssrcs: vec![1215622422],
            },
            None,
        ),
        (
            "Marshal a packet with the maximum possible bitrate.",
            // bitrate = 0xFFFFC00000000000
            // mantissa = 262143 = 0x3FFFF
            // exp = 46
            vec![
                143, 206, 0, 4, 0, 0, 0, 0, 0, 0, 0, 0, 82, 69, 77, 66, 0, 187, 255, 255,
            ],
            ReceiverEstimatedMaximumBitrate {
                sender_ssrc: 0,
                bitrate: 0xFFFFC00000000000,
                ssrcs: vec![],
            },
            None,
        ),
        (
            "Marshal a packet with the overflowed bitrate.",
            // bitrate = 0xFFFFFFFFFFFFFFFF
            // mantissa = 0
            // exp = 47
            vec![
                143, 206, 0, 4, 0, 0, 0, 0, 0, 0, 0, 0, 82, 69, 77, 66, 0, 188, 0, 0,
            ],
            ReceiverEstimatedMaximumBitrate {
                sender_ssrc: 0,
                bitrate: 0xFFFFFFFFFFFFFFFF,
                ssrcs: vec![],
            },
            None,
        ),
    ];

    for (name, data, want, want_error) in tests {
        let mut reader = BufReader::new(data.as_slice());
        let result = ReceiverEstimatedMaximumBitrate::unmarshal(&mut reader);
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
fn test_receiver_estimated_maximum_bitrate_roundtrip() -> Result<(), Error> {
    let tests = vec![
        (
            "Real data sent by Chrome while watching a 6Mb/s stream",
            // mantissa = []byte{26 & 3, 32, 223} = []byte{2, 32, 223} = 139487
            // exp = 26 >> 2 = 6
            // bitrate = 139487 * 2^6 = 139487 * 64 = 8927168 = 8.9 Mb/s
            ReceiverEstimatedMaximumBitrate {
                sender_ssrc: 1,
                bitrate: 8927168,
                ssrcs: vec![1215622422],
            },
            None,
            None,
        ),
        (
            "Marshal a packet with the maximum possible bitrate.",
            // bitrate = 0xFFFFC00000000000
            // mantissa = 262143 = 0x3FFFF
            // exp = 46
            ReceiverEstimatedMaximumBitrate {
                sender_ssrc: 0,
                bitrate: 0xFFFFC00000000000,
                ssrcs: vec![],
            },
            None,
            None,
        ),
        (
            "Marshal a packet with the overflowed bitrate.",
            // bitrate = 0xFFFFFFFFFFFFFFFF
            // mantissa = 0
            // exp = 47
            ReceiverEstimatedMaximumBitrate {
                sender_ssrc: 0,
                bitrate: 0xFFFFFFFFFFFFFFFF,
                ssrcs: vec![],
            },
            None,
            Some(0xFFFFC00000000000u64),
        ),
    ];

    for (name, report, marshal_error, unmarshal_error) in tests {
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
        let decoded = ReceiverEstimatedMaximumBitrate::unmarshal(&mut reader)?;
        if let Some(expected_bitrate) = unmarshal_error {
            assert_eq!(
                decoded.bitrate, expected_bitrate,
                "{} header round trip: got {:?}, want {:?}",
                name, decoded.bitrate, expected_bitrate
            );
        } else {
            assert_eq!(
                decoded, report,
                "{} header round trip: got {:?}, want {:?}",
                name, decoded, report
            );
        }
    }

    Ok(())
}
