use std::io::{BufReader, BufWriter};

use super::*;
use crate::signature_hash_algorithm::*;

#[test]
fn test_handshake_message_certificate_request() -> Result<()> {
    let raw_certificate_request = vec![
        0x02, 0x01, 0x40, 0x00, 0x0C, 0x04, 0x03, 0x04, 0x01, 0x05, 0x03, 0x05, 0x01, 0x06, 0x01,
        0x02, 0x01, 0x00, 0x00,
    ];

    let parsed_certificate_request = HandshakeMessageCertificateRequest {
        certificate_types: vec![
            ClientCertificateType::RsaSign,
            ClientCertificateType::EcdsaSign,
        ],
        signature_hash_algorithms: vec![
            SignatureHashAlgorithm {
                hash: HashAlgorithm::Sha256,
                signature: SignatureAlgorithm::Ecdsa,
            },
            SignatureHashAlgorithm {
                hash: HashAlgorithm::Sha256,
                signature: SignatureAlgorithm::Rsa,
            },
            SignatureHashAlgorithm {
                hash: HashAlgorithm::Sha384,
                signature: SignatureAlgorithm::Ecdsa,
            },
            SignatureHashAlgorithm {
                hash: HashAlgorithm::Sha384,
                signature: SignatureAlgorithm::Rsa,
            },
            SignatureHashAlgorithm {
                hash: HashAlgorithm::Sha512,
                signature: SignatureAlgorithm::Rsa,
            },
            SignatureHashAlgorithm {
                hash: HashAlgorithm::Sha1,
                signature: SignatureAlgorithm::Rsa,
            },
        ],
    };

    let mut reader = BufReader::new(raw_certificate_request.as_slice());
    let c = HandshakeMessageCertificateRequest::unmarshal(&mut reader)?;
    assert_eq!(
        c, parsed_certificate_request,
        "parsedCertificateRequest unmarshal: got {c:?}, want {parsed_certificate_request:?}"
    );

    let mut raw = vec![];
    {
        let mut writer = BufWriter::<&mut Vec<u8>>::new(raw.as_mut());
        c.marshal(&mut writer)?;
    }
    assert_eq!(
        raw, raw_certificate_request,
        "parsedCertificateRequest marshal: got {raw:?}, want {raw_certificate_request:?}"
    );

    Ok(())
}
