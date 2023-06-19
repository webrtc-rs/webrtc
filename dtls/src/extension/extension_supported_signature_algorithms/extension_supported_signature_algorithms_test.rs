use std::io::{BufReader, BufWriter};

use super::*;

#[test]
fn test_extension_supported_signature_algorithms() -> Result<()> {
    let raw_extension_supported_signature_algorithms =
        vec![0x00, 0x08, 0x00, 0x06, 0x04, 0x03, 0x05, 0x03, 0x06, 0x03]; //0x00, 0x0d,
    let parsed_extension_supported_signature_algorithms = ExtensionSupportedSignatureAlgorithms {
        signature_hash_algorithms: vec![
            SignatureHashAlgorithm {
                hash: HashAlgorithm::Sha256,
                signature: SignatureAlgorithm::Ecdsa,
            },
            SignatureHashAlgorithm {
                hash: HashAlgorithm::Sha384,
                signature: SignatureAlgorithm::Ecdsa,
            },
            SignatureHashAlgorithm {
                hash: HashAlgorithm::Sha512,
                signature: SignatureAlgorithm::Ecdsa,
            },
        ],
    };

    let mut raw = vec![];
    {
        let mut writer = BufWriter::<&mut Vec<u8>>::new(raw.as_mut());
        parsed_extension_supported_signature_algorithms.marshal(&mut writer)?;
    }

    assert_eq!(
        raw, raw_extension_supported_signature_algorithms,
        "extensionSupportedSignatureAlgorithms marshal: got {raw:?}, want {raw_extension_supported_signature_algorithms:?}"
    );

    let mut reader = BufReader::new(raw.as_slice());
    let new_extension_supported_signature_algorithms =
        ExtensionSupportedSignatureAlgorithms::unmarshal(&mut reader)?;

    assert_eq!(
        new_extension_supported_signature_algorithms,
        parsed_extension_supported_signature_algorithms,
        "extensionSupportedSignatureAlgorithms unmarshal: got {new_extension_supported_signature_algorithms:?}, want {parsed_extension_supported_signature_algorithms:?}"
    );

    Ok(())
}
