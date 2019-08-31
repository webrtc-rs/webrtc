use super::*;

use std::io::{BufReader, BufWriter};

use util::Error;

#[test]
fn test_header_unmarshal() -> Result<(), Error> {
    let tests = vec![
        (
            "valid",
            vec![
                // v=2, p=0, count=1, RR, len=7
                0x81, 0xc9, 0x00, 0x07,
            ],
            Header {
                padding: false,
                count: 1,
                packet_type: PacketType::ReceiverReport,
                length: 7,
            },
            None,
        ),
        (
            "also valid",
            vec![
                // v=2, p=1, count=1, BYE, len=7
                0xa1, 0xcc, 0x00, 0x07,
            ],
            Header {
                padding: true,
                count: 1,
                packet_type: PacketType::ApplicationDefined,
                length: 7,
            },
            None,
        ),
        (
            "bad version",
            vec![
                // v=0, p=0, count=0, RR, len=4
                0x00, 0xc9, 0x00, 0x04,
            ],
            Header {
                padding: false,
                count: 0,
                packet_type: PacketType::Unsupported,
                length: 0,
            },
            Some(ERR_BAD_VERSION.clone()),
        ),
    ];

    for (name, data, want, want_error) in tests {
        let mut reader = BufReader::new(data.as_slice());
        let result = Header::unmarshal(&mut reader);
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
fn test_header_roundtrip() -> Result<(), Error> {
    let tests = vec![
        (
            "valid",
            Header {
                padding: true,
                count: 31,
                packet_type: PacketType::SenderReport,
                length: 4,
            },
            None,
        ),
        (
            "also valid",
            Header {
                padding: false,
                count: 28,
                packet_type: PacketType::ReceiverReport,
                length: 65535,
            },
            None,
        ),
        (
            "invalid count",
            Header {
                padding: false,
                count: 40,
                packet_type: PacketType::Unsupported,
                length: 0,
            },
            Some(ERR_INVALID_HEADER.clone()),
        ),
    ];

    for (name, header, want_error) in tests {
        let mut data: Vec<u8> = vec![];
        {
            let mut writer = BufWriter::<&mut Vec<u8>>::new(data.as_mut());
            let result = header.marshal(&mut writer);

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
                continue;
            } else {
                assert!(result.is_ok(), "must no error in test {}", name);
            }
        }

        let mut reader = BufReader::new(data.as_slice());
        let decoded = Header::unmarshal(&mut reader)?;
        assert_eq!(
            decoded, header,
            "{} header round trip: got {:?}, want {:?}",
            name, decoded, header
        )
    }

    Ok(())
}
