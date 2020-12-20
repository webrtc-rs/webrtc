#[cfg(test)]
mod test {
    use crate::header::Header;
    use crate::picture_loss_indication::*;

    #[test]
    fn test_picture_loss_indication_unmarshal() {
        let tests = vec![
            (
                "valid",
                vec![
                    0x81, 0xce, 0x00, 0x02, // v=2, p=0, FMT=1, PSFB, len=1
                    0x00, 0x00, 0x00, 0x00, // ssrc=0x0
                    0x4b, 0xc4, 0xfc, 0xb4, // ssrc=0x4bc4fcb4
                ],
                PictureLossIndication {
                    sender_ssrc: 0x0,
                    media_ssrc: 0x4bc4fcb4,
                },
                Ok(()),
            ),
            (
                "packet too short",
                vec![0x81, 0xce, 0x00, 0x00],
                PictureLossIndication::default(),
                Err(ERR_PACKET_TOO_SHORT.clone()),
            ),
            (
                "invalid header",
                vec![
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                ],
                PictureLossIndication::default(),
                Err(ERR_BAD_VERSION.clone()),
            ),
            (
                "wrong type",
                vec![
                    0x81, 0xc9, 0x00, 0x02, // v=2, p=0, FMT=1, RR, len=1
                    0x00, 0x00, 0x00, 0x00, // ssrc=0x0
                    0x4b, 0xc4, 0xfc, 0xb4, // ssrc=0x4bc4fcb4
                ],
                PictureLossIndication::default(),
                Err(ERR_WRONG_TYPE.clone()),
            ),
            (
                "wrong fmt",
                vec![
                    0x82, 0xc9, 0x00, 0x02, // v=2, p=0, FMT=2, RR, len=1
                    0x00, 0x00, 0x00, 0x00, // ssrc=0x0
                    0x4b, 0xc4, 0xfc, 0xb4, // ssrc=0x4bc4fcb4
                ],
                PictureLossIndication::default(),
                Err(ERR_WRONG_TYPE.clone()),
            ),
        ];

        for (name, data, want, want_error) in tests {
            let mut p = PictureLossIndication::default();
            let result = p.unmarshal(&mut data[..].into());

            assert_eq!(
                result, want_error,
                "Unmarshal {} header: err = {:?}, want {:?}",
                name, result, want_error
            );

            match result {
                Err(_) => continue,

                Ok(_) => {
                    assert_eq!(
                        p, want,
                        "Unmarshal {} rr: got {:?}, want {:?}",
                        name, p, want
                    )
                }
            }
        }
    }

    #[test]
    fn test_picture_loss_indication_roundtrip() {
        let tests: Vec<(&str, PictureLossIndication, Result<(), Error>)> = vec![
            (
                "valid",
                PictureLossIndication {
                    sender_ssrc: 1,
                    media_ssrc: 2,
                },
                Ok(()),
            ),
            (
                "also valid",
                PictureLossIndication {
                    sender_ssrc: 5000,
                    media_ssrc: 6000,
                },
                Ok(()),
            ),
        ];

        for (name, report, marshal_error) in tests {
            let data = report.marshal();

            assert_eq!(
                data.is_ok(),
                marshal_error.is_ok(),
                "Marshal {}: err = {:?}, want {:?}",
                name,
                data,
                marshal_error
            );

            match data {
                Err(_) => continue,
                Ok(mut e) => {
                    let mut decoded = PictureLossIndication::default();

                    decoded
                        .unmarshal(&mut e)
                        .expect(format!("Unmarshal {}", name).as_str());

                    assert_eq!(
                        decoded, report,
                        "{} rr round trip: got {:?}, want {:?}",
                        name, decoded, report
                    );
                }
            }
        }
    }

    #[test]
    fn test_picture_loss_indication_unmarshal_header() {
        let test: Vec<(&str, Vec<u8>, Header, Result<(), Error>)> = vec![(
            "valid header",
            vec![
                0x81u8, 0xce, 0x00, 0x02, // v=2, p=0, FMT=1, PSFB, len=1
                0x00, 0x00, 0x00, 0x00, // ssrc=0x0
                0x4b, 0xc4, 0xfc, 0xb4, // ssrc=0x4bc4fcb4
            ],
            Header {
                count: header::FORMAT_PLI,
                packet_type: header::PacketType::PayloadSpecificFeedback,
                length: PLI_LENGTH as u16,
                ..Default::default()
            },
            Ok(()),
        )];

        for (name, bytes, header, want_error) in test {
            let mut pli = PictureLossIndication::default();

            let result = pli.unmarshal(&mut bytes[..].into());

            assert_eq!(
                result, want_error,
                "Unmarshal header {} rr: err = {:?}, want {:?}",
                name, result, want_error
            );

            match result {
                Ok(_) => {
                    assert_eq!(
                        pli.header(),
                        header,
                        "Unmarshal header {} rr: got {:?}, want {:?}",
                        name,
                        pli.header(),
                        header
                    )
                }

                Err(_) => continue,
            }
        }
    }
}
