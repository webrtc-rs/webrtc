use super::*;

use std::io::BufReader;

use util::Error;

#[test]
fn test_goodbye_unmarshal() -> Result<(), Error> {
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
            None,
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
            Some(ERR_PACKET_TOO_SHORT.clone()),
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
            Some(ERR_WRONG_TYPE.clone()),
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
            None,
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
            Some(ERR_FAILED_TO_FILL_WHOLE_BUFFER.clone()),
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
            Some(ERR_FAILED_TO_FILL_WHOLE_BUFFER.clone()),
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
            None,
        ),
        (
            "nil",
            vec![],
            Goodbye {
                sources: vec![],
                reason: "".to_string(),
            },
            Some(ERR_FAILED_TO_FILL_WHOLE_BUFFER.clone()),
        ),
    ];

    for (name, data, want, want_error) in tests {
        let mut reader = BufReader::new(data.as_slice());
        let result = Goodbye::unmarshal(&mut reader);
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
