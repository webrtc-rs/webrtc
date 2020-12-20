#[cfg(test)]
mod test {
    use crate::raw_packet::*;

    #[test]
    fn test_raw_packet_roundtrip() {
        let tests: Vec<(&str, RawPacket, Result<(), Error>, Result<(), Error>)> = vec![
            (
                "valid",
                RawPacket(vec![
                    0x81, 0xcb, 0x00, 0x0c, // v=2, p=0, count=1, BYE, len=12
                    0x90, 0x2f, 0x9e, 0x2e, // ssrc=0x902f9e2e
                    0x03, 0x46, 0x4f, 0x4f, // len=3, text=FOO
                ]),
                Ok(()),
                Ok(()),
            ),
            (
                "short header",
                RawPacket(vec![0x80]),
                Ok(()),
                Err(ERR_FAILED_TO_FILL_WHOLE_BUFFER.clone()),
            ),
            (
                "invalid header",
                RawPacket(
                    // v=0, p=0, count=0, RR, len=4
                    vec![0x00, 0xc9, 0x00, 0x04],
                ),
                Ok(()),
                Err(ERR_BAD_VERSION.clone()),
            ),
        ];

        for (name, pkt, marshal_error, unmarshal_error) in tests {
            let data = pkt.marshal();

            assert_eq!(
                data.is_err(),
                marshal_error.is_err(),
                "Marshal {}: err = {:?}, want {:?}",
                name,
                data,
                marshal_error
            );

            match data {
                Ok(mut e) => {
                    let mut decoded = RawPacket::default();

                    let result = decoded.unmarshal(&mut e);

                    assert_eq!(
                        result.is_err(),
                        unmarshal_error.is_err(),
                        "Unmarshal {}: err = {:?}, want {:?}",
                        name,
                        result,
                        unmarshal_error
                    );

                    if result.is_err() {
                        continue;
                    }

                    assert_eq!(
                        decoded, pkt,
                        "{} raw round trip: got {:?}, want {:?}",
                        name, decoded, pkt
                    )
                }

                Err(_) => continue,
            }
        }
    }
}
