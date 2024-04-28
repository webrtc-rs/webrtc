#[cfg(test)]
mod crypto_test;

pub mod crypto_cbc;
pub mod crypto_ccm;
pub mod crypto_gcm;
pub mod padding;

use std::convert::TryFrom;
use std::sync::Arc;

use der_parser::oid;
use der_parser::oid::Oid;

use rustls::client::danger::ServerCertVerifier;
use rustls::pki_types::{CertificateDer, ServerName};
use rustls::server::danger::ClientCertVerifier;

use rcgen::{generate_simple_self_signed, CertifiedKey, KeyPair};
use ring::rand::SystemRandom;
use ring::signature::{EcdsaKeyPair, Ed25519KeyPair};

use crate::curve::named_curve::*;
use crate::error::*;
use crate::record_layer::record_layer_header::*;
use crate::signature_hash_algorithm::{HashAlgorithm, SignatureAlgorithm, SignatureHashAlgorithm};

/// A X.509 certificate(s) used to authenticate a DTLS connection.
#[derive(Clone, PartialEq, Debug)]
pub struct Certificate {
    /// DER-encoded certificates.
    pub certificate: Vec<CertificateDer<'static>>,
    /// Private key.
    pub private_key: CryptoPrivateKey,
}

impl Certificate {
    /// Generate a self-signed certificate.
    ///
    /// See [`rcgen::generate_simple_self_signed`].
    pub fn generate_self_signed(subject_alt_names: impl Into<Vec<String>>) -> Result<Self> {
        let CertifiedKey { cert, key_pair } =
            generate_simple_self_signed(subject_alt_names).unwrap();
        Ok(Certificate {
            certificate: vec![cert.der().to_owned()],
            private_key: CryptoPrivateKey::try_from(&key_pair)?,
        })
    }

    /// Generate a self-signed certificate with the given algorithm.
    ///
    /// See [`rcgen::Certificate::from_params`].
    pub fn generate_self_signed_with_alg(
        subject_alt_names: impl Into<Vec<String>>,
        alg: &'static rcgen::SignatureAlgorithm,
    ) -> Result<Self> {
        let params = rcgen::CertificateParams::new(subject_alt_names).unwrap();
        let key_pair = rcgen::KeyPair::generate_for(alg).unwrap();
        let cert = params.self_signed(&key_pair).unwrap();

        Ok(Certificate {
            certificate: vec![cert.der().to_owned()],
            private_key: CryptoPrivateKey::try_from(&key_pair)?,
        })
    }

    /// Parses a certificate from the ASCII PEM format.
    #[cfg(feature = "pem")]
    pub fn from_pem(pem_str: &str) -> Result<Self> {
        let mut pems = pem::parse_many(pem_str).map_err(|e| Error::InvalidPEM(e.to_string()))?;
        if pems.len() < 2 {
            return Err(Error::InvalidPEM(format!(
                "expected at least two PEM blocks, got {}",
                pems.len()
            )));
        }
        if pems[0].tag() != "PRIVATE_KEY" {
            return Err(Error::InvalidPEM(format!(
                "invalid tag (expected: 'PRIVATE_KEY', got: '{}')",
                pems[0].tag()
            )));
        }

        let keypair = KeyPair::try_from(pems[0].contents())
            .map_err(|e| Error::InvalidPEM(format!("can't decode keypair: {e}")))?;

        let mut rustls_certs = Vec::new();
        for p in pems.drain(1..) {
            if p.tag() != "CERTIFICATE" {
                return Err(Error::InvalidPEM(format!(
                    "invalid tag (expected: 'CERTIFICATE', got: '{}')",
                    p.tag()
                )));
            }
            rustls_certs.push(CertificateDer::from(p.contents().to_vec()));
        }

        Ok(Certificate {
            certificate: rustls_certs,
            private_key: CryptoPrivateKey::try_from(&keypair)?,
        })
    }

    /// Serializes the certificate (including the private key) in PKCS#8 format in PEM.
    #[cfg(feature = "pem")]
    pub fn serialize_pem(&self) -> String {
        let mut data = vec![pem::Pem::new(
            "PRIVATE_KEY".to_string(),
            self.private_key.serialized_der.clone(),
        )];
        for rustls_cert in &self.certificate {
            data.push(pem::Pem::new(
                "CERTIFICATE".to_string(),
                rustls_cert.as_ref(),
            ));
        }
        pem::encode_many(&data)
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

/// Either ED25519, ECDSA or RSA keypair.
#[derive(Debug)]
pub enum CryptoPrivateKeyKind {
    Ed25519(Ed25519KeyPair),
    Ecdsa256(EcdsaKeyPair),
    Rsa256(ring::rsa::KeyPair),
}

/// Private key.
#[derive(Debug)]
pub struct CryptoPrivateKey {
    /// Keypair.
    pub kind: CryptoPrivateKeyKind,
    /// DER-encoded keypair.
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
                        &SystemRandom::new(),
                    )
                    .unwrap(),
                ),
                serialized_der: self.serialized_der.clone(),
            },
            CryptoPrivateKeyKind::Rsa256(_) => CryptoPrivateKey {
                kind: CryptoPrivateKeyKind::Rsa256(
                    ring::rsa::KeyPair::from_pkcs8(&self.serialized_der).unwrap(),
                ),
                serialized_der: self.serialized_der.clone(),
            },
        }
    }
}

impl TryFrom<&KeyPair> for CryptoPrivateKey {
    type Error = Error;

    fn try_from(key_pair: &KeyPair) -> Result<Self> {
        Self::from_key_pair(key_pair)
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
                        &SystemRandom::new(),
                    )
                    .map_err(|e| Error::Other(e.to_string()))?,
                ),
                serialized_der,
            })
        } else if key_pair.is_compatible(&rcgen::PKCS_RSA_SHA256) {
            Ok(CryptoPrivateKey {
                kind: CryptoPrivateKeyKind::Rsa256(
                    ring::rsa::KeyPair::from_pkcs8(&serialized_der)
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
            let mut signature = vec![0; kp.public().modulus_len()];
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
    insecure_verification: bool,
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
        SignatureAlgorithm::Rsa if (hash_algorithm.hash == HashAlgorithm::Sha256) => {
            if remote_key_signature.len() < 256 && insecure_verification {
                &ring::signature::RSA_PKCS1_1024_8192_SHA256_FOR_LEGACY_USE_ONLY
            } else {
                &ring::signature::RSA_PKCS1_2048_8192_SHA256
            }
        }
        SignatureAlgorithm::Rsa if hash_algorithm.hash == HashAlgorithm::Sha384 => {
            &ring::signature::RSA_PKCS1_2048_8192_SHA384
        }
        SignatureAlgorithm::Rsa if hash_algorithm.hash == HashAlgorithm::Sha512 => {
            if remote_key_signature.len() < 256 && insecure_verification {
                &ring::signature::RSA_PKCS1_1024_8192_SHA512_FOR_LEGACY_USE_ONLY
            } else {
                &ring::signature::RSA_PKCS1_2048_8192_SHA512
            }
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
    insecure_verification: bool,
) -> Result<()> {
    verify_signature(
        message,
        hash_algorithm,
        remote_key_signature,
        raw_certificates,
        insecure_verification,
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
            let mut signature = vec![0; kp.public().modulus_len()];
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
    insecure_verification: bool,
) -> Result<()> {
    verify_signature(
        handshake_bodies,
        hash_algorithm,
        remote_key_signature,
        raw_certificates,
        insecure_verification,
    )
}

pub(crate) fn load_certs(raw_certificates: &[Vec<u8>]) -> Result<Vec<CertificateDer<'static>>> {
    if raw_certificates.is_empty() {
        return Err(Error::ErrLengthMismatch);
    }

    let mut certs = vec![];
    for raw_cert in raw_certificates {
        let cert = CertificateDer::from(raw_cert.to_vec());
        certs.push(cert);
    }

    Ok(certs)
}

pub(crate) fn verify_client_cert(
    raw_certificates: &[Vec<u8>],
    cert_verifier: &Arc<dyn ClientCertVerifier>,
) -> Result<Vec<CertificateDer<'static>>> {
    let chains = load_certs(raw_certificates)?;

    let (end_entity, intermediates) = chains
        .split_first()
        .ok_or(Error::ErrClientCertificateRequired)?;

    match cert_verifier.verify_client_cert(
        end_entity,
        intermediates,
        rustls::pki_types::UnixTime::now(),
    ) {
        Ok(_) => {}
        Err(err) => return Err(Error::Other(err.to_string())),
    };

    Ok(chains)
}

pub(crate) fn verify_server_cert(
    raw_certificates: &[Vec<u8>],
    cert_verifier: &Arc<dyn ServerCertVerifier>,
    server_name: &str,
) -> Result<Vec<CertificateDer<'static>>> {
    let chains = load_certs(raw_certificates)?;
    let server_name = match ServerName::try_from(server_name) {
        Ok(server_name) => server_name,
        Err(err) => return Err(Error::Other(err.to_string())),
    };

    let (end_entity, intermediates) = chains
        .split_first()
        .ok_or(Error::ErrServerMustHaveCertificate)?;
    match cert_verifier.verify_server_cert(
        end_entity,
        intermediates,
        &server_name,
        &[],
        rustls::pki_types::UnixTime::now(),
    ) {
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

#[cfg(test)]
mod test {
    #[cfg(feature = "pem")]
    use super::*;

    #[cfg(feature = "pem")]
    #[test]
    fn test_certificate_serialize_pem_and_from_pem() -> crate::error::Result<()> {
        let cert = Certificate::generate_self_signed(vec!["webrtc.rs".to_owned()])?;

        let pem = cert.serialize_pem();
        let loaded_cert = Certificate::from_pem(&pem)?;

        assert_eq!(loaded_cert, cert);

        Ok(())
    }
}
