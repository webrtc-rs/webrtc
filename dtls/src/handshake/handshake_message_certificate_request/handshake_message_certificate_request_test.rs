use super::*;
use crate::signature_hash_algorithm::*;

use std::io::{BufReader, BufWriter};

use util::Error;

#[test]
fn test_handshake_message_certificate_request() -> Result<(), Error> {
    let raw_certificate_request = vec![
        0x02, 0x01, 0x40, 0x00, 0x0C, 0x04, 0x03, 0x04, 0x01, 0x05, 0x03, 0x05, 0x01, 0x06, 0x01,
        0x02, 0x01, 0x00, 0x00,
    ];

    let parsed_certificate_request = HandshakeMessageCertificateRequest {
        certificate_types: vec![
            ClientCertificateType::RSASign,
            ClientCertificateType::ECDSASign,
        ],
        signature_hash_algorithms: vec![
            SignatureHashAlgorithm {
                hash: HashAlgorithm::SHA256,
                signature: SignatureAlgorithm::ECDSA,
            },
            SignatureHashAlgorithm {
                hash: HashAlgorithm::SHA256,
                signature: SignatureAlgorithm::RSA,
            },
            SignatureHashAlgorithm {
                hash: HashAlgorithm::SHA384,
                signature: SignatureAlgorithm::ECDSA,
            },
            SignatureHashAlgorithm {
                hash: HashAlgorithm::SHA384,
                signature: SignatureAlgorithm::RSA,
            },
            SignatureHashAlgorithm {
                hash: HashAlgorithm::SHA512,
                signature: SignatureAlgorithm::RSA,
            },
            SignatureHashAlgorithm {
                hash: HashAlgorithm::SHA1,
                signature: SignatureAlgorithm::RSA,
            },
        ],
    };

    let mut reader = BufReader::new(raw_certificate_request.as_slice());
    let c = HandshakeMessageCertificateRequest::unmarshal(&mut reader)?;
    assert_eq!(
        c, parsed_certificate_request,
        "parsedCertificateRequest unmarshal: got {:?}, want {:?}",
        c, parsed_certificate_request
    );

    let mut raw = vec![];
    {
        let mut writer = BufWriter::<&mut Vec<u8>>::new(raw.as_mut());
        c.marshal(&mut writer)?;
    }
    assert_eq!(
        raw, raw_certificate_request,
        "parsedCertificateRequest marshal: got {:?}, want {:?}",
        raw, raw_certificate_request
    );

    Ok(())
}
