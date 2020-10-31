#[cfg(test)]
mod crypto_test;

pub mod crypto_cbc;
pub mod crypto_ccm;
pub mod crypto_gcm;

use crate::curve::named_curve::*;
use crate::errors::*;
use crate::record_layer::record_layer_header::*;

use der_parser::{oid, oid::Oid};

use util::Error;

use rsa::PublicKey;
use sha2::{Digest, Sha256};
use signature::{Signature, Verifier};

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

pub(crate) enum CryptoPrivateKey {
    ED25519(Box<dyn ed25519::signature::Signer<ed25519::Signature>>),
    ECDSA256(Box<dyn p256::ecdsa::signature::Signer<p256::ecdsa::Signature>>),
    RSA256(rsa::RSAPrivateKey),
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
    private_key: CryptoPrivateKey, /*, hash_algorithm: HashAlgorithm*/
) -> Result<Vec<u8>, Error> {
    let msg = value_key_message(client_random, server_random, public_key, named_curve);
    let signature = match &private_key {
        CryptoPrivateKey::ED25519(p) => p.sign(&msg).to_bytes().to_vec(),
        CryptoPrivateKey::ECDSA256(p) => p.sign(&msg).as_bytes().to_vec(),
        CryptoPrivateKey::RSA256(p) => {
            let padding =
                rsa::padding::PaddingScheme::new_pkcs1v15_sign(Some(rsa::hash::Hash::SHA2_256));
            let mut hasher = Sha256::new();
            hasher.update(msg);
            let hashed = hasher.finalize();
            p.sign(padding, hashed.as_slice())?
        }
    };

    Ok(signature)
}

// add OID_ED25519 which is not defined in x509_parser
pub const OID_ED25519: Oid<'static> = oid!(1.3.101.112);
pub const OID_ECDSA: Oid<'static> = oid!(1.2.840.10045.2.1);

pub(crate) fn verify_key_signature(
    message: &[u8],
    remote_key_signature: &[u8],
    /*_hash_algorithm: HashAlgorithm,*/
    raw_certificates: &[u8],
) -> Result<(), Error> {
    if raw_certificates.len() == 0 {
        return Err(ERR_LENGTH_MISMATCH.clone());
    }

    let (_, certificate) = x509_parser::parse_x509_der(raw_certificates)?;

    let pki_alg = &certificate.tbs_certificate.subject_pki.algorithm.algorithm;
    if *pki_alg == OID_ED25519 {
        let public_key = ed25519_dalek::PublicKey::from_bytes(
            certificate
                .tbs_certificate
                .subject_pki
                .subject_public_key
                .data,
        )?;
        let signature = ed25519_dalek::Signature::from_bytes(remote_key_signature)?;
        public_key.verify(message, &signature)?;
    } else if *pki_alg == OID_ECDSA {
        let public_key = p256::ecdsa::VerifyKey::new(
            certificate
                .tbs_certificate
                .subject_pki
                .subject_public_key
                .data,
        )?;
        let signature = p256::ecdsa::Signature::from_asn1(remote_key_signature)?;
        public_key.verify(message, &signature)?;
    } else if *pki_alg == x509_parser::objects::OID_RSA_ENCRYPTION {
        let sign_alg = &certificate.tbs_certificate.signature.algorithm;

        //*sign_alg ==  x509_parser::objects::OID_RSA_SHA1 ||
        //*sign_alg ==  x509_parser::objects::OID_RSA_SHA384 ||
        //*sign_alg ==  x509_parser::objects::OID_RSA_SHA512 ||
        if *sign_alg == x509_parser::objects::OID_RSA_SHA256 {
            let public_key = rsa::RSAPublicKey::from_pkcs1(
                certificate
                    .tbs_certificate
                    .subject_pki
                    .subject_public_key
                    .data,
            )?;
            let padding =
                rsa::padding::PaddingScheme::new_pkcs1v15_sign(Some(rsa::hash::Hash::SHA2_256));
            let mut hasher = Sha256::new();
            hasher.update(message);
            let hashed = hasher.finalize();
            public_key.verify(padding, hashed.as_slice(), remote_key_signature)?;
        } else {
            return Err(ERR_KEY_SIGNATURE_VERIFY_UNIMPLEMENTED.clone());
        }
    } else {
        return Err(ERR_KEY_SIGNATURE_VERIFY_UNIMPLEMENTED.clone());
    }

    Ok(())
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
    private_key: CryptoPrivateKey, /*, hashAlgorithm hashAlgorithm*/
) -> Result<Vec<u8>, Error> {
    let mut h = Sha256::new();
    h.update(handshake_bodies);
    let hashed = h.finalize();

    let signature = match &private_key {
        CryptoPrivateKey::ED25519(p) => p.sign(hashed.as_slice()).to_bytes().to_vec(),
        CryptoPrivateKey::ECDSA256(p) => p.sign(hashed.as_slice()).as_bytes().to_vec(),
        CryptoPrivateKey::RSA256(p) => {
            let padding =
                rsa::padding::PaddingScheme::new_pkcs1v15_sign(Some(rsa::hash::Hash::SHA2_256));
            p.sign(padding, hashed.as_slice())?
        }
    };

    Ok(signature)
}

pub(crate) fn verify_certificate_verify(
    handshake_bodies: &[u8],
    /*hashAlgorithm hashAlgorithm,*/
    remote_key_signature: &[u8],
    raw_certificates: &[u8],
) -> Result<(), Error> {
    if raw_certificates.len() == 0 {
        return Err(ERR_LENGTH_MISMATCH.clone());
    }

    let (_, certificate) = x509_parser::parse_x509_der(raw_certificates)?;

    let pki_alg = &certificate.tbs_certificate.subject_pki.algorithm.algorithm;
    if *pki_alg == OID_ED25519 {
        let public_key = ed25519_dalek::PublicKey::from_bytes(
            certificate
                .tbs_certificate
                .subject_pki
                .subject_public_key
                .data,
        )?;
        let signature = ed25519_dalek::Signature::from_bytes(remote_key_signature)?;
        public_key.verify(handshake_bodies, &signature)?;
    } else if *pki_alg == OID_ECDSA {
        let public_key = p256::ecdsa::VerifyKey::new(
            certificate
                .tbs_certificate
                .subject_pki
                .subject_public_key
                .data,
        )?;
        let signature = p256::ecdsa::Signature::from_asn1(remote_key_signature)?;
        public_key.verify(handshake_bodies, &signature)?;
    } else if *pki_alg == x509_parser::objects::OID_RSA_ENCRYPTION {
        let sign_alg = &certificate.tbs_certificate.signature.algorithm;

        //*sign_alg ==  x509_parser::objects::OID_RSA_SHA1 ||
        //*sign_alg ==  x509_parser::objects::OID_RSA_SHA384 ||
        //*sign_alg ==  x509_parser::objects::OID_RSA_SHA512 ||
        if *sign_alg == x509_parser::objects::OID_RSA_SHA256 {
            let public_key = rsa::RSAPublicKey::from_pkcs1(
                certificate
                    .tbs_certificate
                    .subject_pki
                    .subject_public_key
                    .data,
            )?;
            let padding =
                rsa::padding::PaddingScheme::new_pkcs1v15_sign(Some(rsa::hash::Hash::SHA2_256));
            let mut hasher = Sha256::new();
            hasher.update(handshake_bodies);
            let hashed = hasher.finalize();
            public_key.verify(padding, hashed.as_slice(), remote_key_signature)?;
        } else {
            return Err(ERR_KEY_SIGNATURE_VERIFY_UNIMPLEMENTED.clone());
        }
    } else {
        return Err(ERR_KEY_SIGNATURE_VERIFY_UNIMPLEMENTED.clone());
    }

    Ok(())
}

pub(crate) fn load_certs(
    raw_certificates: &[u8],
) -> Result<x509_parser::X509Certificate<'_>, Error> {
    if raw_certificates.len() == 0 {
        return Err(ERR_LENGTH_MISMATCH.clone());
    }

    let (_, certificate) = x509_parser::parse_x509_der(raw_certificates)?;

    Ok(certificate)
}

pub(crate) fn verify_cert(raw_certificates: &[u8]) -> Result<(), Error> {
    let certificate = load_certs(raw_certificates)?;

    certificate.verify_signature(None)?;

    Ok(())
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
