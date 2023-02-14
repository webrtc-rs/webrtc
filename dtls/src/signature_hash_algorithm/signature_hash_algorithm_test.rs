use super::*;

#[test]
fn test_parse_signature_schemes() -> Result<()> {
    let tests = vec![
        (
            "Translate",
            vec![
                SignatureScheme::EcdsaWithP256AndSha256 as u16,
                SignatureScheme::EcdsaWithP384AndSha384 as u16,
                SignatureScheme::EcdsaWithP521AndSha512 as u16,
                SignatureScheme::Pkcs1WithSha256 as u16,
                SignatureScheme::Pkcs1WithSha384 as u16,
                SignatureScheme::Pkcs1WithSha512 as u16,
            ],
            vec![
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
                SignatureHashAlgorithm {
                    hash: HashAlgorithm::Sha256,
                    signature: SignatureAlgorithm::Rsa,
                },
                SignatureHashAlgorithm {
                    hash: HashAlgorithm::Sha384,
                    signature: SignatureAlgorithm::Rsa,
                },
                SignatureHashAlgorithm {
                    hash: HashAlgorithm::Sha512,
                    signature: SignatureAlgorithm::Rsa,
                },
            ],
            false,
            None,
        ),
        (
            "InvalidSignatureAlgorithm",
            vec![
                SignatureScheme::EcdsaWithP256AndSha256 as u16, // Valid
                0x04FF, // Invalid: unknown signature with SHA-256
            ],
            vec![],
            false,
            Some(Error::ErrInvalidSignatureAlgorithm),
        ),
        (
            "InvalidHashAlgorithm",
            vec![
                SignatureScheme::EcdsaWithP256AndSha256 as u16, // Valid
                0x0003,                                         // Invalid: ECDSA with MD2
            ],
            vec![],
            false,
            Some(Error::ErrInvalidHashAlgorithm),
        ),
        (
            "InsecureHashAlgorithmDenied",
            vec![
                SignatureScheme::EcdsaWithP256AndSha256 as u16, // Valid
                SignatureScheme::EcdsaWithSha1 as u16,          // Insecure
            ],
            vec![SignatureHashAlgorithm {
                hash: HashAlgorithm::Sha256,
                signature: SignatureAlgorithm::Ecdsa,
            }],
            false,
            None,
        ),
        (
            "InsecureHashAlgorithmAllowed",
            vec![
                SignatureScheme::EcdsaWithP256AndSha256 as u16, // Valid
                SignatureScheme::EcdsaWithSha1 as u16,          // Insecure
            ],
            vec![
                SignatureHashAlgorithm {
                    hash: HashAlgorithm::Sha256,
                    signature: SignatureAlgorithm::Ecdsa,
                },
                SignatureHashAlgorithm {
                    hash: HashAlgorithm::Sha1,
                    signature: SignatureAlgorithm::Ecdsa,
                },
            ],
            true,
            None,
        ),
        (
            "OnlyInsecureHashAlgorithm",
            vec![
                SignatureScheme::EcdsaWithSha1 as u16, // Insecure
            ],
            vec![],
            false,
            Some(Error::ErrNoAvailableSignatureSchemes),
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
                    err.to_string(),
                    output_err.to_string(),
                    "Expected error: {err:?}, got: {output_err:?}"
                );
            } else {
                panic!("expect err, but got non-err for {name}");
            }
        } else if let Ok(output_val) = output {
            assert_eq!(
                expected, output_val,
                "Expected signatureHashAlgorithm:\n{expected:?}\ngot:\n{output_val:?}",
            );
        } else {
            panic!("expect non-err, but got err for {name}");
        }
    }

    Ok(())
}
