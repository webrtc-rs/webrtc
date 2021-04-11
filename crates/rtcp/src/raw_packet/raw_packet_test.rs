use super::*;

#[test]
fn test_raw_packet_roundtrip() {
    let tests: Vec<(&str, RawPacket, Result<(), Error>, Result<(), Error>)> = vec![
        (
            "valid",
            RawPacket(Bytes::from_static(&[
                0x81, 0xcb, 0x00, 0x0c, // v=2, p=0, count=1, BYE, len=12
                0x90, 0x2f, 0x9e, 0x2e, // ssrc=0x902f9e2e
                0x03, 0x46, 0x4f, 0x4f, // len=3, text=FOO
            ])),
            Ok(()),
            Ok(()),
        ),
        (
            "short header",
            RawPacket(Bytes::from_static(&[0x80])),
            Ok(()),
            Err(Error::PacketTooShort),
        ),
        (
            "invalid header",
            RawPacket(
                // v=0, p=0, count=0, RR, len=4
                Bytes::from_static(&[0x00, 0xc9, 0x00, 0x04]),
            ),
            Ok(()),
            Err(Error::BadVersion),
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
            Ok(e) => {
                let result = RawPacket::unmarshal(&e);

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

                let decoded = result.unwrap();

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
