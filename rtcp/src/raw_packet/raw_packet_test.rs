use super::*;
use crate::errors::*;

use std::io::{BufReader, BufWriter};

use util::Error;

#[test]
fn test_raw_packet_roundtrip() -> Result<(), Error> {
    let tests = vec![
        (
            "valid",
            RawPacket {
                raw: vec![
                    // v=2, p=0, count=1, BYE, len=12
                    0x81, 0xcb, 0x00, 0x0c, // ssrc=0x902f9e2e
                    0x90, 0x2f, 0x9e, 0x2e, // len=3, text=FOO
                    0x03, 0x46, 0x4f, 0x4f,
                ],
                header: Header {
                    padding: false,
                    count: 1,
                    packet_type: PacketType::Goodbye,
                    length: 12,
                },
            },
            None,
            None,
        ),
        (
            "short header",
            RawPacket {
                raw: vec![0x80],
                ..Default::default()
            },
            None,
            Some(ERR_FAILED_TO_FILL_WHOLE_BUFFER.clone()),
        ),
        (
            "invalid header",
            RawPacket {
                raw: vec![
                    // v=0, p=0, count=0, RR, len=4
                    0x00, 0xc9, 0x00, 0x04,
                ],
                ..Default::default()
            },
            None,
            Some(ERR_BAD_VERSION.clone()),
        ),
    ];

    for (name, pkt, marshal_error, unmarshal_error) in tests {
        let mut data: Vec<u8> = vec![];
        {
            let mut writer = BufWriter::<&mut Vec<u8>>::new(data.as_mut());
            let result = pkt.marshal(&mut writer);
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
        let result = RawPacket::unmarshal(&mut reader);
        if let Some(err) = unmarshal_error {
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
            let decoded = result.unwrap();
            assert_eq!(
                decoded, pkt,
                "{} header round trip: got {:?}, want {:?}",
                name, decoded, pkt
            )
        }
    }

    Ok(())
}
