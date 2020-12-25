use super::*;

use std::io::{BufReader, BufWriter};

use util::Error;

#[test]
fn test_picture_loss_indication_unmarshal() -> Result<(), Error> {
    let tests = vec![
        (
            "valid",
            vec![
                // v=2, p=0, FMT=1, PSFB, len=1
                0x81, 0xce, 0x00, 0x02, // ssrc=0x0
                0x00, 0x00, 0x00, 0x00, // ssrc=0x4bc4fcb4
                0x4b, 0xc4, 0xfc, 0xb4,
            ],
            PictureLossIndication {
                sender_ssrc: 0x0,
                media_ssrc: 0x4bc4fcb4,
            },
            None,
        ),
        (
            "packet too short",
            vec![0x81, 0xce, 0x00, 0x00],
            PictureLossIndication::default(),
            Some(ERR_FAILED_TO_FILL_WHOLE_BUFFER.clone()),
        ),
        (
            "invalid header",
            vec![
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            ],
            PictureLossIndication::default(),
            Some(ERR_BAD_VERSION.clone()),
        ),
        (
            "wrong type",
            vec![
                // v=2, p=0, FMT=1, RR, len=1
                0x81, 0xc9, 0x00, 0x02, // ssrc=0x0
                0x00, 0x00, 0x00, 0x00, // ssrc=0x4bc4fcb4
                0x4b, 0xc4, 0xfc, 0xb4,
            ],
            PictureLossIndication::default(),
            Some(ERR_WRONG_TYPE.clone()),
        ),
        (
            "wrong fmt",
            vec![
                // v=2, p=0, FMT=2, RR, len=1
                0x82, 0xc9, 0x00, 0x02, // ssrc=0x0
                0x00, 0x00, 0x00, 0x00, // ssrc=0x4bc4fcb4
                0x4b, 0xc4, 0xfc, 0xb4,
            ],
            PictureLossIndication::default(),
            Some(ERR_WRONG_TYPE.clone()),
        ),
    ];

    for (name, data, want, want_error) in tests {
        let mut reader = BufReader::new(data.as_slice());
        let result = PictureLossIndication::unmarshal(&mut reader);
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
fn test_picture_loss_indication_roundtrip() -> Result<(), Error> {
    let tests = vec![
        (
            "valid",
            PictureLossIndication {
                sender_ssrc: 1,
                media_ssrc: 2,
            },
            None,
        ),
        (
            "also valid",
            PictureLossIndication {
                sender_ssrc: 5000,
                media_ssrc: 6000,
            },
            None,
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
        let decoded = PictureLossIndication::unmarshal(&mut reader)?;
        assert_eq!(
            decoded, report,
            "{} header round trip: got {:?}, want {:?}",
            name, decoded, report
        )
    }

    Ok(())
}
