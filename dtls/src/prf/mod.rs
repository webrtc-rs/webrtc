#[cfg(test)]
mod prf_test;

use std::convert::TryInto;
use std::fmt;

use hmac::{Hmac, Mac};
use sha1::Sha1;
use sha2::{Digest, Sha256};

type HmacSha256 = Hmac<Sha256>;
type HmacSha1 = Hmac<Sha1>;

use crate::cipher_suite::CipherSuiteHash;
use crate::content::ContentType;
use crate::curve::named_curve::*;
use crate::error::*;
use crate::record_layer::record_layer_header::ProtocolVersion;

pub(crate) const PRF_MASTER_SECRET_LABEL: &str = "master secret";
pub(crate) const PRF_EXTENDED_MASTER_SECRET_LABEL: &str = "extended master secret";
pub(crate) const PRF_KEY_EXPANSION_LABEL: &str = "key expansion";
pub(crate) const PRF_VERIFY_DATA_CLIENT_LABEL: &str = "client finished";
pub(crate) const PRF_VERIFY_DATA_SERVER_LABEL: &str = "server finished";

#[derive(PartialEq, Debug, Clone)]
pub(crate) struct EncryptionKeys {
    pub(crate) master_secret: Vec<u8>,
    pub(crate) client_mac_key: Vec<u8>,
    pub(crate) server_mac_key: Vec<u8>,
    pub(crate) client_write_key: Vec<u8>,
    pub(crate) server_write_key: Vec<u8>,
    pub(crate) client_write_iv: Vec<u8>,
    pub(crate) server_write_iv: Vec<u8>,
}

impl fmt::Display for EncryptionKeys {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = "EncryptionKeys:\n".to_string();

        out += format!("- master_secret: {:?}\n", self.master_secret).as_str();
        out += format!("- client_mackey: {:?}\n", self.client_mac_key).as_str();
        out += format!("- server_mackey: {:?}\n", self.server_mac_key).as_str();
        out += format!("- client_write_key: {:?}\n", self.client_write_key).as_str();
        out += format!("- server_write_key: {:?}\n", self.server_write_key).as_str();
        out += format!("- client_write_iv: {:?}\n", self.client_write_iv).as_str();
        out += format!("- server_write_iv: {:?}\n", self.server_write_iv).as_str();

        write!(f, "{out}")
    }
}

// The premaster secret is formed as follows: if the PSK is N octets
// long, concatenate a uint16 with the value N, N zero octets, a second
// uint16 with the value N, and the PSK itself.
//
// https://tools.ietf.org/html/rfc4279#section-2
pub(crate) fn prf_psk_pre_master_secret(psk: &[u8]) -> Vec<u8> {
    let psk_len = psk.len();

    let mut out = vec![0u8; 2 + psk_len + 2];

    out.extend_from_slice(psk);
    let be = (psk_len as u16).to_be_bytes();
    out[..2].copy_from_slice(&be);
    out[2 + psk_len..2 + psk_len + 2].copy_from_slice(&be);

    out
}

pub(crate) fn prf_pre_master_secret(
    public_key: &[u8],
    private_key: &NamedCurvePrivateKey,
    curve: NamedCurve,
) -> Result<Vec<u8>> {
    match curve {
        NamedCurve::P256 => elliptic_curve_pre_master_secret(public_key, private_key, curve),
        NamedCurve::P384 => elliptic_curve_pre_master_secret(public_key, private_key, curve),
        NamedCurve::X25519 => elliptic_curve_pre_master_secret(public_key, private_key, curve),
        _ => Err(Error::ErrInvalidNamedCurve),
    }
}

fn elliptic_curve_pre_master_secret(
    public_key: &[u8],
    private_key: &NamedCurvePrivateKey,
    curve: NamedCurve,
) -> Result<Vec<u8>> {
    match curve {
        NamedCurve::P256 => {
            let pub_key = p256::EncodedPoint::from_bytes(public_key)?;
            let public = p256::PublicKey::from_sec1_bytes(pub_key.as_ref())?;
            if let NamedCurvePrivateKey::EphemeralSecretP256(secret) = private_key {
                return Ok(secret.diffie_hellman(&public).raw_secret_bytes().to_vec());
            }
        }
        NamedCurve::P384 => {
            let pub_key = p384::EncodedPoint::from_bytes(public_key)?;
            let public = p384::PublicKey::from_sec1_bytes(pub_key.as_ref())?;
            if let NamedCurvePrivateKey::EphemeralSecretP384(secret) = private_key {
                return Ok(secret.diffie_hellman(&public).raw_secret_bytes().to_vec());
            }
        }
        NamedCurve::X25519 => {
            if public_key.len() != 32 {
                return Err(Error::Other("Public key is not 32 len".into()));
            }
            let pub_key: [u8; 32] = public_key.try_into().unwrap();
            let public = x25519_dalek::PublicKey::from(pub_key);
            if let NamedCurvePrivateKey::StaticSecretX25519(secret) = private_key {
                return Ok(secret.diffie_hellman(&public).as_bytes().to_vec());
            }
        }
        _ => return Err(Error::ErrInvalidNamedCurve),
    }
    Err(Error::ErrNamedCurveAndPrivateKeyMismatch)
}

//  This PRF with the SHA-256 hash function is used for all cipher suites
//  defined in this document and in TLS documents published prior to this
//  document when TLS 1.2 is negotiated.  New cipher suites MUST explicitly
//  specify a PRF and, in general, SHOULD use the TLS PRF with SHA-256 or a
//  stronger standard hash function.
//
//     P_hash(secret, seed) = HMAC_hash(secret, A(1) + seed) +
//                            HMAC_hash(secret, A(2) + seed) +
//                            HMAC_hash(secret, A(3) + seed) + ...
//
//  A() is defined as:
//
//     A(0) = seed
//     A(i) = HMAC_hash(secret, A(i-1))
//
//  P_hash can be iterated as many times as necessary to produce the
//  required quantity of data.  For example, if P_SHA256 is being used to
//  create 80 bytes of data, it will have to be iterated three times
//  (through A(3)), creating 96 bytes of output data; the last 16 bytes
//  of the final iteration will then be discarded, leaving 80 bytes of
//  output data.
//
// https://tools.ietf.org/html/rfc4346w
fn hmac_sha(h: CipherSuiteHash, key: &[u8], data: &[u8]) -> Result<Vec<u8>> {
    let mut mac = match h {
        CipherSuiteHash::Sha256 => {
            HmacSha256::new_from_slice(key).map_err(|e| Error::Other(e.to_string()))?
        }
    };
    mac.update(data);
    let result = mac.finalize();
    let code_bytes = result.into_bytes();
    Ok(code_bytes.to_vec())
}

pub(crate) fn prf_p_hash(
    secret: &[u8],
    seed: &[u8],
    requested_length: usize,
    h: CipherSuiteHash,
) -> Result<Vec<u8>> {
    let mut last_round = seed.to_vec();
    let mut out = vec![];

    let iterations = ((requested_length as f64) / (h.size() as f64)).ceil() as usize;
    for _ in 0..iterations {
        last_round = hmac_sha(h, secret, &last_round)?;

        let mut last_round_seed = last_round.clone();
        last_round_seed.extend_from_slice(seed);
        let with_secret = hmac_sha(h, secret, &last_round_seed)?;

        out.extend_from_slice(&with_secret);
    }

    Ok(out[..requested_length].to_vec())
}

pub(crate) fn prf_extended_master_secret(
    pre_master_secret: &[u8],
    session_hash: &[u8],
    h: CipherSuiteHash,
) -> Result<Vec<u8>> {
    let mut seed = PRF_EXTENDED_MASTER_SECRET_LABEL.as_bytes().to_vec();
    seed.extend_from_slice(session_hash);
    prf_p_hash(pre_master_secret, &seed, 48, h)
}

pub(crate) fn prf_master_secret(
    pre_master_secret: &[u8],
    client_random: &[u8],
    server_random: &[u8],
    h: CipherSuiteHash,
) -> Result<Vec<u8>> {
    let mut seed = PRF_MASTER_SECRET_LABEL.as_bytes().to_vec();
    seed.extend_from_slice(client_random);
    seed.extend_from_slice(server_random);
    prf_p_hash(pre_master_secret, &seed, 48, h)
}

pub(crate) fn prf_encryption_keys(
    master_secret: &[u8],
    client_random: &[u8],
    server_random: &[u8],
    prf_mac_len: usize,
    prf_key_len: usize,
    prf_iv_len: usize,
    h: CipherSuiteHash,
) -> Result<EncryptionKeys> {
    let mut seed = PRF_KEY_EXPANSION_LABEL.as_bytes().to_vec();
    seed.extend_from_slice(server_random);
    seed.extend_from_slice(client_random);

    let material = prf_p_hash(
        master_secret,
        &seed,
        (2 * prf_mac_len) + (2 * prf_key_len) + (2 * prf_iv_len),
        h,
    )?;
    let mut key_material = &material[..];

    let client_mac_key = key_material[..prf_mac_len].to_vec();
    key_material = &key_material[prf_mac_len..];

    let server_mac_key = key_material[..prf_mac_len].to_vec();
    key_material = &key_material[prf_mac_len..];

    let client_write_key = key_material[..prf_key_len].to_vec();
    key_material = &key_material[prf_key_len..];

    let server_write_key = key_material[..prf_key_len].to_vec();
    key_material = &key_material[prf_key_len..];

    let client_write_iv = key_material[..prf_iv_len].to_vec();
    key_material = &key_material[prf_iv_len..];

    let server_write_iv = key_material[..prf_iv_len].to_vec();

    Ok(EncryptionKeys {
        master_secret: master_secret.to_vec(),
        client_mac_key,
        server_mac_key,
        client_write_key,
        server_write_key,
        client_write_iv,
        server_write_iv,
    })
}

pub(crate) fn prf_verify_data(
    master_secret: &[u8],
    handshake_bodies: &[u8],
    label: &str,
    h: CipherSuiteHash,
) -> Result<Vec<u8>> {
    let mut hasher = match h {
        CipherSuiteHash::Sha256 => Sha256::new(),
    };
    hasher.update(handshake_bodies);
    let result = hasher.finalize();
    let mut seed = label.as_bytes().to_vec();
    seed.extend_from_slice(&result);

    prf_p_hash(master_secret, &seed, 12, h)
}

pub(crate) fn prf_verify_data_client(
    master_secret: &[u8],
    handshake_bodies: &[u8],
    h: CipherSuiteHash,
) -> Result<Vec<u8>> {
    prf_verify_data(
        master_secret,
        handshake_bodies,
        PRF_VERIFY_DATA_CLIENT_LABEL,
        h,
    )
}

pub(crate) fn prf_verify_data_server(
    master_secret: &[u8],
    handshake_bodies: &[u8],
    h: CipherSuiteHash,
) -> Result<Vec<u8>> {
    prf_verify_data(
        master_secret,
        handshake_bodies,
        PRF_VERIFY_DATA_SERVER_LABEL,
        h,
    )
}

// compute the MAC using HMAC-SHA1
pub(crate) fn prf_mac(
    epoch: u16,
    sequence_number: u64,
    content_type: ContentType,
    protocol_version: ProtocolVersion,
    payload: &[u8],
    key: &[u8],
) -> Result<Vec<u8>> {
    let mut hmac = HmacSha1::new_from_slice(key).map_err(|e| Error::Other(e.to_string()))?;

    let mut msg = vec![0u8; 13];
    msg[..2].copy_from_slice(&epoch.to_be_bytes());
    msg[2..8].copy_from_slice(&sequence_number.to_be_bytes()[2..]);
    msg[8] = content_type as u8;
    msg[9] = protocol_version.major;
    msg[10] = protocol_version.minor;
    msg[11..].copy_from_slice(&(payload.len() as u16).to_be_bytes());

    hmac.update(&msg);
    hmac.update(payload);
    let result = hmac.finalize();

    Ok(result.into_bytes().to_vec())
}
