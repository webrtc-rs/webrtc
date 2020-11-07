use super::*;

use util::Error;

#[test]
fn test_parse_signature_schemes() -> Result<(), Error> {
    let tests = vec![
        (
            "Translate",
            vec![
                SignatureScheme::ECDSAWithP256AndSHA256 as u16,
                SignatureScheme::ECDSAWithP384AndSHA384 as u16,
                SignatureScheme::ECDSAWithP521AndSHA512 as u16,
                SignatureScheme::PKCS1WithSHA256 as u16,
                SignatureScheme::PKCS1WithSHA384 as u16,
                SignatureScheme::PKCS1WithSHA512 as u16,
            ],
            vec![
                SignatureHashAlgorithm {
                    hash: HashAlgorithm::SHA256,
                    signature: SignatureAlgorithm::ECDSA,
                },
                SignatureHashAlgorithm {
                    hash: HashAlgorithm::SHA384,
                    signature: SignatureAlgorithm::ECDSA,
                },
                SignatureHashAlgorithm {
                    hash: HashAlgorithm::SHA512,
                    signature: SignatureAlgorithm::ECDSA,
                },
                SignatureHashAlgorithm {
                    hash: HashAlgorithm::SHA256,
                    signature: SignatureAlgorithm::RSA,
                },
                SignatureHashAlgorithm {
                    hash: HashAlgorithm::SHA384,
                    signature: SignatureAlgorithm::RSA,
                },
                SignatureHashAlgorithm {
                    hash: HashAlgorithm::SHA512,
                    signature: SignatureAlgorithm::RSA,
                },
            ],
            false,
            None,
        ),
        (
            "InvalidSignatureAlgorithm",
            vec![
                SignatureScheme::ECDSAWithP256AndSHA256 as u16, // Valid
                0x04FF, // Invalid: unknown signature with SHA-256
            ],
            vec![],
            false,
            Some(ERR_INVALID_SIGNATURE_ALGORITHM.clone()),
        ),
        (
            "InvalidHashAlgorithm",
            vec![
                SignatureScheme::ECDSAWithP256AndSHA256 as u16, // Valid
                0x0003,                                         // Invalid: ECDSA with MD2
            ],
            vec![],
            false,
            Some(ERR_INVALID_HASH_ALGORITHM.clone()),
        ),
        (
            "InsecureHashAlgorithmDenied",
            vec![
                SignatureScheme::ECDSAWithP256AndSHA256 as u16, // Valid
                SignatureScheme::ECDSAWithSHA1 as u16,          // Insecure
            ],
            vec![SignatureHashAlgorithm {
                hash: HashAlgorithm::SHA256,
                signature: SignatureAlgorithm::ECDSA,
            }],
            false,
            None,
        ),
        (
            "InsecureHashAlgorithmAllowed",
            vec![
                SignatureScheme::ECDSAWithP256AndSHA256 as u16, // Valid
                SignatureScheme::ECDSAWithSHA1 as u16,          // Insecure
            ],
            vec![
                SignatureHashAlgorithm {
                    hash: HashAlgorithm::SHA256,
                    signature: SignatureAlgorithm::ECDSA,
                },
                SignatureHashAlgorithm {
                    hash: HashAlgorithm::SHA1,
                    signature: SignatureAlgorithm::ECDSA,
                },
            ],
            true,
            None,
        ),
        (
            "OnlyInsecureHashAlgorithm",
            vec![
                SignatureScheme::ECDSAWithSHA1 as u16, // Insecure
            ],
            vec![],
            false,
            Some(ERR_NO_AVAILABLE_SIGNATURE_SCHEMES.clone()),
        ),
        (
            "Translate",
            vec![SignatureScheme::Ed25519 as u16],
            vec![SignatureHashAlgorithm {
                hash: HashAlgorithm::Ed25519,
                signature: SignatureAlgorithm::Ed25519,
            }],
            false,
            None,
        ),
    ];

    for (name, inputs, expected, insecure_hashes, want_err) in tests {
        let output = parse_signature_schemes(&inputs, insecure_hashes);
        if let Some(err) = want_err {
            if let Err(output_err) = output {
                assert_eq!(
                    err, output_err,
                    "Expected error: {:?}, got: {:?}",
                    err, output_err
                );
            } else {
                assert!(false, "expect err, but got non-err for {}", name);
            }
        } else if let Ok(output_val) = output {
            assert_eq!(
                expected, output_val,
                "Expected signatureHashAlgorithm:\n{:?}\ngot:\n{:?}",
                expected, output_val,
            );
        } else {
            assert!(false, "expect non-err, but got err for {}", name);
        }
    }

    Ok(())
}
