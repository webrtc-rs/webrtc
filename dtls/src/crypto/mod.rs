#[cfg(test)]
mod crypto_test;

pub mod crypto_cbc;
pub mod crypto_ccm;
pub mod crypto_gcm;
pub mod padding;

use crate::curve::named_curve::*;
use crate::error::*;
use crate::record_layer::record_layer_header::*;
use crate::signature_hash_algorithm::{HashAlgorithm, SignatureAlgorithm, SignatureHashAlgorithm};

use der_parser::{oid, oid::Oid};
use rcgen::KeyPair;
use ring::rand::SystemRandom;
use ring::signature::{EcdsaKeyPair, Ed25519KeyPair, RsaKeyPair};
use std::sync::Arc;

#[derive(Clone, PartialEq)]
pub struct Certificate {
    pub certificate: Vec<rustls::Certificate>,
    pub private_key: CryptoPrivateKey,
}

impl Certificate {
    pub fn generate_self_signed(subject_alt_names: impl Into<Vec<String>>) -> Result<Self> {
        let cert = rcgen::generate_simple_self_signed(subject_alt_names)?;
        let certificate = cert.serialize_der()?;
        let key_pair = cert.get_key_pair();
        let serialized_der = key_pair.serialize_der();
        let private_key = if key_pair.is_compatible(&rcgen::PKCS_ED25519) {
            CryptoPrivateKey {
                kind: CryptoPrivateKeyKind::Ed25519(
                    Ed25519KeyPair::from_pkcs8(&serialized_der)
                        .map_err(|e| Error::Other(e.to_string()))?,
                ),
                serialized_der,
            }
        } else if key_pair.is_compatible(&rcgen::PKCS_ECDSA_P256_SHA256) {
            CryptoPrivateKey {
                kind: CryptoPrivateKeyKind::Ecdsa256(
                    EcdsaKeyPair::from_pkcs8(
                        &ring::signature::ECDSA_P256_SHA256_ASN1_SIGNING,
                        &serialized_der,
                    )
                    .map_err(|e| Error::Other(e.to_string()))?,
                ),
                serialized_der,
            }
        } else if key_pair.is_compatible(&rcgen::PKCS_RSA_SHA256) {
            CryptoPrivateKey {
                kind: CryptoPrivateKeyKind::Rsa256(
                    RsaKeyPair::from_pkcs8(&serialized_der)
                        .map_err(|e| Error::Other(e.to_string()))?,
                ),
                serialized_der,
            }
        } else {
            return Err(Error::Other("Unsupported key_pair".to_owned()));
        };

        Ok(Certificate {
            certificate: vec![rustls::Certificate(certificate)],
            private_key,
        })
    }

    pub fn generate_self_signed_with_alg(
        subject_alt_names: impl Into<Vec<String>>,
        alg: &'static rcgen::SignatureAlgorithm,
    ) -> Result<Self> {
        let mut params = rcgen::CertificateParams::new(subject_alt_names);
        params.alg = alg;
        let cert = rcgen::Certificate::from_params(params)?;
        let certificate = cert.serialize_der()?;
        let key_pair = cert.get_key_pair();
        let serialized_der = key_pair.serialize_der();
        let private_key = if key_pair.is_compatible(&rcgen::PKCS_ED25519) {
            CryptoPrivateKey {
                kind: CryptoPrivateKeyKind::Ed25519(
                    Ed25519KeyPair::from_pkcs8(&serialized_der)
                        .map_err(|e| Error::Other(e.to_string()))?,
                ),
                serialized_der,
            }
        } else if key_pair.is_compatible(&rcgen::PKCS_ECDSA_P256_SHA256) {
            CryptoPrivateKey {
                kind: CryptoPrivateKeyKind::Ecdsa256(
                    EcdsaKeyPair::from_pkcs8(
                        &ring::signature::ECDSA_P256_SHA256_ASN1_SIGNING,
                        &serialized_der,
                    )
                    .map_err(|e| Error::Other(e.to_string()))?,
                ),
                serialized_der,
            }
        } else if key_pair.is_compatible(&rcgen::PKCS_RSA_SHA256) {
            CryptoPrivateKey {
                kind: CryptoPrivateKeyKind::Rsa256(
                    RsaKeyPair::from_pkcs8(&serialized_der)
                        .map_err(|e| Error::Other(e.to_string()))?,
                ),
                serialized_der,
            }
        } else {
            return Err(Error::Other("Unsupported key_pair".to_owned()));
        };

        Ok(Certificate {
            certificate: vec![rustls::Certificate(certificate)],
            private_key,
        })
    }
}

pub(crate) fn value_key_message(
    client_random: &[u8],
    server_random: &[u8],
    public_key: &[u8],
    named_curve: NamedCurve,
) -> Vec<u8> {
    let mut server_ecdh_params = vec![0u8; 4];
    server_ecdh_params[0] = 3; // named curve
    server_ecdh_params[1..3].copy_from_slice(&(named_curve as u16).to_be_bytes());
    server_ecdh_params[3] = public_key.len() as u8;

    let mut plaintext = vec![];
    plaintext.extend_from_slice(client_random);
    plaintext.extend_from_slice(server_random);
    plaintext.extend_from_slice(&server_ecdh_params);
    plaintext.extend_from_slice(public_key);

    plaintext
}

pub enum CryptoPrivateKeyKind {
    Ed25519(Ed25519KeyPair),
    Ecdsa256(EcdsaKeyPair),
    Rsa256(RsaKeyPair),
}

pub struct CryptoPrivateKey {
    pub kind: CryptoPrivateKeyKind,
    pub serialized_der: Vec<u8>,
}

impl PartialEq for CryptoPrivateKey {
    fn eq(&self, other: &Self) -> bool {
        if self.serialized_der != other.serialized_der {
            return false;
        }

        matches!(
            (&self.kind, &other.kind),
            (
                CryptoPrivateKeyKind::Rsa256(_),
                CryptoPrivateKeyKind::Rsa256(_)
            ) | (
                CryptoPrivateKeyKind::Ecdsa256(_),
                CryptoPrivateKeyKind::Ecdsa256(_)
            ) | (
                CryptoPrivateKeyKind::Ed25519(_),
                CryptoPrivateKeyKind::Ed25519(_)
            )
        )
    }
}

impl Clone for CryptoPrivateKey {
    fn clone(&self) -> Self {
        match self.kind {
            CryptoPrivateKeyKind::Ed25519(_) => CryptoPrivateKey {
                kind: CryptoPrivateKeyKind::Ed25519(
                    Ed25519KeyPair::from_pkcs8(&self.serialized_der).unwrap(),
                ),
                serialized_der: self.serialized_der.clone(),
            },
            CryptoPrivateKeyKind::Ecdsa256(_) => CryptoPrivateKey {
                kind: CryptoPrivateKeyKind::Ecdsa256(
                    EcdsaKeyPair::from_pkcs8(
                        &ring::signature::ECDSA_P256_SHA256_ASN1_SIGNING,
                        &self.serialized_der,
                    )
                    .unwrap(),
                ),
                serialized_der: self.serialized_der.clone(),
            },
            CryptoPrivateKeyKind::Rsa256(_) => CryptoPrivateKey {
                kind: CryptoPrivateKeyKind::Rsa256(
                    RsaKeyPair::from_pkcs8(&self.serialized_der).unwrap(),
                ),
                serialized_der: self.serialized_der.clone(),
            },
        }
    }
}

impl CryptoPrivateKey {
    pub fn from_key_pair(key_pair: &KeyPair) -> Result<Self> {
        let serialized_der = key_pair.serialize_der();
        if key_pair.is_compatible(&rcgen::PKCS_ED25519) {
            Ok(CryptoPrivateKey {
                kind: CryptoPrivateKeyKind::Ed25519(
                    Ed25519KeyPair::from_pkcs8(&serialized_der)
                        .map_err(|e| Error::Other(e.to_string()))?,
                ),
                serialized_der,
            })
        } else if key_pair.is_compatible(&rcgen::PKCS_ECDSA_P256_SHA256) {
            Ok(CryptoPrivateKey {
                kind: CryptoPrivateKeyKind::Ecdsa256(
                    EcdsaKeyPair::from_pkcs8(
                        &ring::signature::ECDSA_P256_SHA256_ASN1_SIGNING,
                        &serialized_der,
                    )
                    .map_err(|e| Error::Other(e.to_string()))?,
                ),
                serialized_der,
            })
        } else if key_pair.is_compatible(&rcgen::PKCS_RSA_SHA256) {
            Ok(CryptoPrivateKey {
                kind: CryptoPrivateKeyKind::Rsa256(
                    RsaKeyPair::from_pkcs8(&serialized_der)
                        .map_err(|e| Error::Other(e.to_string()))?,
                ),
                serialized_der,
            })
        } else {
            Err(Error::Other("Unsupported key_pair".to_owned()))
        }
    }
}

// If the client provided a "signature_algorithms" extension, then all
// certificates provided by the server MUST be signed by a
// hash/signature algorithm pair that appears in that extension
//
// https://tools.ietf.org/html/rfc5246#section-7.4.2
pub(crate) fn generate_key_signature(
    client_random: &[u8],
    server_random: &[u8],
    public_key: &[u8],
    named_curve: NamedCurve,
    private_key: &CryptoPrivateKey, /*, hash_algorithm: HashAlgorithm*/
) -> Result<Vec<u8>> {
    let msg = value_key_message(client_random, server_random, public_key, named_curve);
    let signature = match &private_key.kind {
        CryptoPrivateKeyKind::Ed25519(kp) => kp.sign(&msg).as_ref().to_vec(),
        CryptoPrivateKeyKind::Ecdsa256(kp) => {
            let system_random = SystemRandom::new();
            kp.sign(&system_random, &msg)
                .map_err(|e| Error::Other(e.to_string()))?
                .as_ref()
                .to_vec()
        }
        CryptoPrivateKeyKind::Rsa256(kp) => {
            let system_random = SystemRandom::new();
            let mut signature = vec![0; kp.public_modulus_len()];
            kp.sign(
                &ring::signature::RSA_PKCS1_SHA256,
                &system_random,
                &msg,
                &mut signature,
            )
            .map_err(|e| Error::Other(e.to_string()))?;

            signature
        }
    };

    Ok(signature)
}

// add OID_ED25519 which is not defined in x509_parser
pub const OID_ED25519: Oid<'static> = oid!(1.3.101 .112);
pub const OID_ECDSA: Oid<'static> = oid!(1.2.840 .10045 .2 .1);

fn verify_signature(
    message: &[u8],
    hash_algorithm: &SignatureHashAlgorithm,
    remote_key_signature: &[u8],
    raw_certificates: &[Vec<u8>],
) -> Result<()> {
    if raw_certificates.is_empty() {
        return Err(Error::ErrLengthMismatch);
    }

    let (_, certificate) = x509_parser::parse_x509_certificate(&raw_certificates[0])
        .map_err(|e| Error::Other(e.to_string()))?;

    let verify_alg: &dyn ring::signature::VerificationAlgorithm = match hash_algorithm.signature {
        SignatureAlgorithm::Ed25519 => &ring::signature::ED25519,
        SignatureAlgorithm::Ecdsa if hash_algorithm.hash == HashAlgorithm::Sha256 => {
            &ring::signature::ECDSA_P256_SHA256_ASN1
        }
        SignatureAlgorithm::Ecdsa if hash_algorithm.hash == HashAlgorithm::Sha384 => {
            &ring::signature::ECDSA_P384_SHA384_ASN1
        }
        SignatureAlgorithm::Rsa if hash_algorithm.hash == HashAlgorithm::Sha1 => {
            &ring::signature::RSA_PKCS1_1024_8192_SHA1_FOR_LEGACY_USE_ONLY
        }
        SignatureAlgorithm::Rsa if hash_algorithm.hash == HashAlgorithm::Sha256 => {
            &ring::signature::RSA_PKCS1_2048_8192_SHA256
        }
        SignatureAlgorithm::Rsa if hash_algorithm.hash == HashAlgorithm::Sha384 => {
            &ring::signature::RSA_PKCS1_2048_8192_SHA384
        }
        SignatureAlgorithm::Rsa if hash_algorithm.hash == HashAlgorithm::Sha512 => {
            &ring::signature::RSA_PKCS1_2048_8192_SHA512
        }
        _ => return Err(Error::ErrKeySignatureVerifyUnimplemented),
    };

    log::trace!("Picked an algorithm {:?}", verify_alg);

    let public_key = ring::signature::UnparsedPublicKey::new(
        verify_alg,
        certificate
            .tbs_certificate
            .subject_pki
            .subject_public_key
            .data,
    );

    public_key
        .verify(message, remote_key_signature)
        .map_err(|e| Error::Other(e.to_string()))?;

    Ok(())
}

pub(crate) fn verify_key_signature(
    message: &[u8],
    hash_algorithm: &SignatureHashAlgorithm,
    remote_key_signature: &[u8],
    raw_certificates: &[Vec<u8>],
) -> Result<()> {
    verify_signature(
        message,
        hash_algorithm,
        remote_key_signature,
        raw_certificates,
    )
}

// If the server has sent a CertificateRequest message, the client MUST send the Certificate
// message.  The ClientKeyExchange message is now sent, and the content
// of that message will depend on the public key algorithm selected
// between the ClientHello and the ServerHello.  If the client has sent
// a certificate with signing ability, a digitally-signed
// CertificateVerify message is sent to explicitly verify possession of
// the private key in the certificate.
// https://tools.ietf.org/html/rfc5246#section-7.3
pub(crate) fn generate_certificate_verify(
    handshake_bodies: &[u8],
    private_key: &CryptoPrivateKey, /*, hashAlgorithm hashAlgorithm*/
) -> Result<Vec<u8>> {
    let signature = match &private_key.kind {
        CryptoPrivateKeyKind::Ed25519(kp) => kp.sign(handshake_bodies).as_ref().to_vec(),
        CryptoPrivateKeyKind::Ecdsa256(kp) => {
            let system_random = SystemRandom::new();
            kp.sign(&system_random, handshake_bodies)
                .map_err(|e| Error::Other(e.to_string()))?
                .as_ref()
                .to_vec()
        }
        CryptoPrivateKeyKind::Rsa256(kp) => {
            let system_random = SystemRandom::new();
            let mut signature = vec![0; kp.public_modulus_len()];
            kp.sign(
                &ring::signature::RSA_PKCS1_SHA256,
                &system_random,
                handshake_bodies,
                &mut signature,
            )
            .map_err(|e| Error::Other(e.to_string()))?;

            signature
        }
    };

    Ok(signature)
}

pub(crate) fn verify_certificate_verify(
    handshake_bodies: &[u8],
    hash_algorithm: &SignatureHashAlgorithm,
    remote_key_signature: &[u8],
    raw_certificates: &[Vec<u8>],
) -> Result<()> {
    verify_signature(
        handshake_bodies,
        hash_algorithm,
        remote_key_signature,
        raw_certificates,
    )
}

pub(crate) fn load_certs(raw_certificates: &[Vec<u8>]) -> Result<Vec<rustls::Certificate>> {
    if raw_certificates.is_empty() {
        return Err(Error::ErrLengthMismatch);
    }

    let mut certs = vec![];
    for raw_cert in raw_certificates {
        let cert = rustls::Certificate(raw_cert.to_vec());
        certs.push(cert);
    }

    Ok(certs)
}

pub(crate) fn verify_client_cert(
    raw_certificates: &[Vec<u8>],
    cert_verifier: &Arc<dyn rustls::ClientCertVerifier>,
) -> Result<Vec<rustls::Certificate>> {
    let chains = load_certs(raw_certificates)?;

    match cert_verifier.verify_client_cert(&chains, None) {
        Ok(_) => {}
        Err(err) => return Err(Error::Other(err.to_string())),
    };

    Ok(chains)
}

pub(crate) fn verify_server_cert(
    raw_certificates: &[Vec<u8>],
    cert_verifier: &Arc<dyn rustls::ServerCertVerifier>,
    roots: &rustls::RootCertStore,
    server_name: &str,
) -> Result<Vec<rustls::Certificate>> {
    let chains = load_certs(raw_certificates)?;
    let dns_name = match webpki::DNSNameRef::try_from_ascii_str(server_name) {
        Ok(dns_name) => dns_name,
        Err(err) => return Err(Error::Other(err.to_string())),
    };

    match cert_verifier.verify_server_cert(roots, &chains, dns_name, &[]) {
        Ok(_) => {}
        Err(err) => return Err(Error::Other(err.to_string())),
    };

    Ok(chains)
}

pub(crate) fn generate_aead_additional_data(h: &RecordLayerHeader, payload_len: usize) -> Vec<u8> {
    let mut additional_data = vec![0u8; 13];
    // SequenceNumber MUST be set first
    // we only want uint48, clobbering an extra 2 (using uint64, rust doesn't have uint48)
    additional_data[..8].copy_from_slice(&h.sequence_number.to_be_bytes());
    additional_data[..2].copy_from_slice(&h.epoch.to_be_bytes());
    additional_data[8] = h.content_type as u8;
    additional_data[9] = h.protocol_version.major;
    additional_data[10] = h.protocol_version.minor;
    additional_data[11..].copy_from_slice(&(payload_len as u16).to_be_bytes());

    additional_data
}
