#[cfg(test)]
mod test {
    use crate::errors::*;
    use crate::header::*;

    #[test]
    fn test_header_unmarshal() {
        let tests = vec![
            (
                "valid",
                vec![
                    // v=2, p=0, count=1, RR, len=7
                    0x81u8, 0xc9, 0x00, 0x07,
                ],
                Header {
                    padding: false,
                    count: 1,
                    packet_type: PacketType::ReceiverReport,
                    length: 7,
                },
                Ok(()),
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
                Ok(()),
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
                Err(ERR_BAD_VERSION.clone()),
            ),
        ];

        for (name, data, want, want_error) in tests {
            let mut h = Header::default();

            let got_error = h.unmarshal(&mut data.as_slice().into());

            assert_eq!(
                got_error, want_error,
                "Unmarshal {} header: err = {:?}, want {:?}",
                name, got_error, want_error
            );

            match got_error {
                Ok(_) => {
                    assert_eq!(
                        h, want,
                        "Unmarshal {} header: got {:?}, want {:?}",
                        name, h, want
                    )
                }

                Err(_) => continue,
            }
        }
    }

    #[test]
    fn test_header_roundtrip() {
        let tests = vec![
            (
                "valid",
                Header {
                    padding: true,
                    count: 31,
                    packet_type: PacketType::SenderReport,
                    length: 4,
                },
                Ok(()),
            ),
            (
                "also valid",
                Header {
                    padding: false,
                    count: 28,
                    packet_type: PacketType::ReceiverReport,
                    length: 65535,
                },
                Ok(()),
            ),
            (
                "invalid count",
                Header {
                    padding: false,
                    count: 40,
                    packet_type: PacketType::Unsupported,
                    length: 0,
                },
                Err(ERR_INVALID_HEADER.clone()),
            ),
        ];

        for (name, header, want_error) in tests {
            let data = header.marshal();

            assert_eq!(
                data.clone().err(),
                want_error.clone().err(),
                "Marshal {}: err = {:?}, want {:?}",
                name,
                data,
                want_error
            );

            match data {
                Ok(mut e) => {
                    let mut decoded = Header::default();

                    decoded
                        .unmarshal(&mut e)
                        .expect(format!("Unmarshal {}", name).as_str());

                    assert_eq!(
                        decoded, header,
                        "{} header round trip: got {:?}, want {:?}",
                        name, decoded, header
                    )
                }

                Err(_) => continue,
            }
        }
    }
}
