pub mod crypto_cbc;
pub mod crypto_ccm;
pub mod crypto_gcm;

use crate::curve::named_curve::*;
use crate::errors::*;
use crate::record_layer::record_layer_header::*;
use crate::signature_hash_algorithm::*;

use util::Error;

use signature::Signature;

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
            p.sign(padding, &msg)?
        }
    };

    Ok(signature)
}

pub(crate) fn verify_key_signature(
    _message: &[u8],
    _remote_key_signature: &[u8],
    _hash_algorithm: HashAlgorithm,
    raw_certificates: &[u8],
) -> Result<(), Error> {
    if raw_certificates.len() == 0 {
        return Err(ERR_LENGTH_MISMATCH.clone());
    }

    let res = x509_parser::parse_x509_der(raw_certificates);

    let (_rem, _certificate) = match res {
        Ok((rem, cert)) => (rem, cert),
        Err(err) => return Err(Error::new(err.to_string())),
    };

    //TODO:

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
