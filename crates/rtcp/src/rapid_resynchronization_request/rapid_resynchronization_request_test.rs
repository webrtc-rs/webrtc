#[cfg(test)]
mod test {
    use crate::{errors::Error, rapid_resynchronization_request::*};

    #[test]
    fn test_rapid_resynchronization_request_unmarshal() {
        let tests = vec![
            (
                "valid",
                vec![
                    0x85, 0xcd, 0x0, 0x2, // RapidResynchronizationRequest
                    0x90, 0x2f, 0x9e, 0x2e, // sender=0x902f9e2e
                    0x90, 0x2f, 0x9e, 0x2e, // media=0x902f9e2e
                ],
                RapidResynchronizationRequest {
                    sender_ssrc: 0x902f9e2e,
                    media_ssrc: 0x902f9e2e,
                },
                Ok(()),
            ),
            (
                "short report",
                vec![
                    0x85, 0xcd, 0x0, 0x2, // ssrc=0x902f9e2e
                    0x90, 0x2f, 0x9e, 0x2e,
                    // report ends early
                ],
                RapidResynchronizationRequest::default(),
                Err(Error::PacketTooShort),
            ),
            (
                "wrong type",
                vec![
                    0x81, 0xc8, 0x0, 0x7, // v=2, p=0, count=1, SR, len=7
                    0x90, 0x2f, 0x9e, 0x2e, // ssrc=0x902f9e2e
                    0xbc, 0x5e, 0x9a, 0x40, // ssrc=0xbc5e9a40
                    0x0, 0x0, 0x0, 0x0, // fracLost=0, totalLost=0
                    0x0, 0x0, 0x46, 0xe1, // lastSeq=0x46e1
                    0x0, 0x0, 0x1, 0x11, // jitter=273
                    0x9, 0xf3, 0x64, 0x32, // lsr=0x9f36432
                    0x0, 0x2, 0x4a, 0x79, // delay=150137
                ],
                RapidResynchronizationRequest::default(),
                Err(Error::WrongType),
            ),
            (
                "nil",
                vec![],
                RapidResynchronizationRequest::default(),
                Err(Error::PacketTooShort),
            ),
        ];

        for (name, data, want, want_error) in tests {
            let mut rrr = RapidResynchronizationRequest::default();

            let result = rrr.unmarshal(&mut data[..].into());

            assert_eq!(
                result, want_error,
                "Unmarshal {} rr: err = {:?}, want {:?}",
                name, result, want_error
            );

            match result {
                Ok(_) => assert_eq!(
                    rrr, want,
                    "Unmarshal {} rr: got {:?}, want {:?}",
                    name, rrr, want
                ),

                Err(_) => continue,
            }
        }
    }

    #[test]
    fn test_rapid_resynchronization_request_roundtrip() {
        let tests: Vec<(&str, RapidResynchronizationRequest, Result<(), Error>)> = vec![(
            "valid",
            RapidResynchronizationRequest {
                sender_ssrc: 0x902f9e2e,
                media_ssrc: 0x902f9e2e,
            },
            Ok(()),
        )];

        for (name, report, want_error) in tests {
            let data = report.marshal();

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
                    let mut decoded = RapidResynchronizationRequest::default();

                    decoded
                        .unmarshal(&mut e)
                        .expect(format!("Unmarshal error {}", name).as_str());

                    assert_eq!(
                        decoded, report,
                        "{} rrr round trip: got {:?}, want {:?}",
                        name, decoded, report
                    )
                }

                Err(_) => continue,
            }
        }
    }
}
