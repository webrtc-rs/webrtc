use std::io::{BufReader, BufWriter};

use super::*;

#[test]
fn test_handshake_message_certificate_request() -> Result<()> {
    let raw_certificate_verify = vec![
        0x04, 0x03, 0x00, 0x47, 0x30, 0x45, 0x02, 0x20, 0x6b, 0x63, 0x17, 0xad, 0xbe, 0xb7, 0x7b,
        0x0f, 0x86, 0x73, 0x39, 0x1e, 0xba, 0xb3, 0x50, 0x9c, 0xce, 0x9c, 0xe4, 0x8b, 0xe5, 0x13,
        0x07, 0x59, 0x18, 0x1f, 0xe5, 0xa0, 0x2b, 0xca, 0xa6, 0xad, 0x02, 0x21, 0x00, 0xd3, 0xb5,
        0x01, 0xbe, 0x87, 0x6c, 0x04, 0xa1, 0xdc, 0x28, 0xaa, 0x5f, 0xf7, 0x1e, 0x9c, 0xc0, 0x1e,
        0x00, 0x2c, 0xe5, 0x94, 0xbb, 0x03, 0x0e, 0xf1, 0xcb, 0x28, 0x22, 0x33, 0x23, 0x88, 0xad,
    ];
    let parsed_certificate_verify = HandshakeMessageCertificateVerify {
        algorithm: SignatureHashAlgorithm {
            hash: raw_certificate_verify[0].into(),
            signature: raw_certificate_verify[1].into(),
        },
        signature: raw_certificate_verify[4..].to_vec(),
    };

    let mut reader = BufReader::new(raw_certificate_verify.as_slice());
    let c = HandshakeMessageCertificateVerify::unmarshal(&mut reader)?;
    assert_eq!(
        c, parsed_certificate_verify,
        "handshakeMessageCertificate unmarshal: got {c:?}, want {parsed_certificate_verify:?}"
    );

    let mut raw = vec![];
    {
        let mut writer = BufWriter::<&mut Vec<u8>>::new(raw.as_mut());
        c.marshal(&mut writer)?;
    }
    assert_eq!(
        raw, raw_certificate_verify,
        "handshakeMessageCertificateVerify marshal: got {raw:?}, want {raw_certificate_verify:?}"
    );

    Ok(())
}
