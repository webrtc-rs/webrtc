use std::io::{BufReader, BufWriter};

use super::*;

#[test]
fn test_handshake_message_hello_verify_request() -> Result<()> {
    let raw_hello_verify_request = vec![
        0xfe, 0xff, 0x14, 0x25, 0xfb, 0xee, 0xb3, 0x7c, 0x95, 0xcf, 0x00, 0xeb, 0xad, 0xe2, 0xef,
        0xc7, 0xfd, 0xbb, 0xed, 0xf7, 0x1f, 0x6c, 0xcd,
    ];
    let parsed_hello_verify_request = HandshakeMessageHelloVerifyRequest {
        version: ProtocolVersion {
            major: 0xFE,
            minor: 0xFF,
        },
        cookie: vec![
            0x25, 0xfb, 0xee, 0xb3, 0x7c, 0x95, 0xcf, 0x00, 0xeb, 0xad, 0xe2, 0xef, 0xc7, 0xfd,
            0xbb, 0xed, 0xf7, 0x1f, 0x6c, 0xcd,
        ],
    };

    let mut reader = BufReader::new(raw_hello_verify_request.as_slice());
    let c = HandshakeMessageHelloVerifyRequest::unmarshal(&mut reader)?;
    assert_eq!(
        c, parsed_hello_verify_request,
        "parsed_hello_verify_request unmarshal: got {c:?}, want {parsed_hello_verify_request:?}"
    );

    let mut raw = vec![];
    {
        let mut writer = BufWriter::<&mut Vec<u8>>::new(raw.as_mut());
        c.marshal(&mut writer)?;
    }
    assert_eq!(
        raw, raw_hello_verify_request,
        "parsed_hello_verify_request marshal: got {raw:?}, want {raw_hello_verify_request:?}"
    );

    Ok(())
}
