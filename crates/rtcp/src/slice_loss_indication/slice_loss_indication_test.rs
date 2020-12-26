#[cfg(test)]
mod test {
    use crate::slice_loss_indication::*;

    #[test]
    fn test_slice_loss_indication_unmarshal() {
        let tests = vec![
            (
                "valid",
                vec![
                    0x82u8, 0xcd, 0x0, 0x3, // SliceLossIndication
                    0x90, 0x2f, 0x9e, 0x2e, // sender=0x902f9e2e
                    0x90, 0x2f, 0x9e, 0x2e, // media=0x902f9e2e
                    0x55, 0x50, 0x00, 0x2C, // nack 0xAAAA, 0x5555
                ],
                SliceLossIndication {
                    sender_ssrc: 0x902f9e2e,
                    media_ssrc: 0x902f9e2e,
                    sli_entries: vec![SLIEntry {
                        first: 0xaaa,
                        number: 0,
                        picture: 0x2C,
                    }],
                },
                None,
            ),
            (
                "short report",
                vec![
                    0x82, 0xcd, 0x0, 0x2, // ssrc=0x902f9e2e
                    0x90, 0x2f, 0x9e, 0x2e,
                    // report ends early
                ],
                SliceLossIndication::default(),
                Some(ERR_PACKET_TOO_SHORT.clone()),
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
                SliceLossIndication::default(),
                Some(ERR_WRONG_TYPE.clone()),
            ),
            (
                "nil",
                vec![],
                SliceLossIndication::default(),
                Some(ERR_PACKET_TOO_SHORT.clone()),
            ),
        ];

        for (name, data, want, want_error) in tests {
            let mut sli = SliceLossIndication::default();

            let result = sli.unmarshal(&mut data[..].into());

            assert_eq!(
                result.clone().err(),
                want_error,
                "Unmarshal {} rr: err = {:?}, want {:?}",
                name,
                result,
                want_error
            );

            match result {
                Ok(_) => {
                    assert_eq!(
                        sli, want,
                        "Unmarshal {} rr: got {:#?}, want {:#?}",
                        name, sli, want
                    )
                }

                Err(_) => continue,
            }
        }
    }

    #[test]
    fn test_slice_loss_indication_roundtrip() {
        let tests: Vec<(&str, SliceLossIndication, Option<Error>)> = vec![(
            "valid",
            SliceLossIndication {
                sender_ssrc: 0x902f9e2e,
                media_ssrc: 0x902f9e2e,
                sli_entries: vec![
                    SLIEntry {
                        first: 1,
                        number: 0xAA,
                        picture: 0x1F,
                    },
                    SLIEntry {
                        first: 1034,
                        number: 0x05,
                        picture: 0x6,
                    },
                ],
            },
            None,
        )];

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
                    let mut decoded = SliceLossIndication::default();

                    decoded
                        .unmarshal(&mut e)
                        .expect(format!("Unmarshal {}", name).as_str());

                    assert_eq!(
                        decoded, report,
                        "{} sli round trip: got {:#?}, want {:#?}",
                        name, decoded, report
                    )
                }

                Err(_) => continue,
            }
        }
    }
}
