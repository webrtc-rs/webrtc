use std::fmt;

use util::Error;

use crate::curve::named_curve::*;
use crate::errors::*;

pub(crate) const PRF_MASTER_SECRET_LABEL: &'static str = "master secret";
pub(crate) const PRF_EXTENDED_MASTER_SECRET_LABEL: &'static str = "extended master secret";
pub(crate) const PRF_KEY_EXPANSION_LABEL: &'static str = "key expansion";
pub(crate) const PRF_VERIFY_DATA_CLIENT_LABEL: &'static str = "client finished";
pub(crate) const PRF_VERIFY_DATA_SERVER_LABEL: &'static str = "server finished";

pub(crate) struct EncryptionKeys {
    master_secret: Vec<u8>,
    client_mac_key: Vec<u8>,
    server_mac_key: Vec<u8>,
    client_write_key: Vec<u8>,
    server_write_key: Vec<u8>,
    client_write_iv: Vec<u8>,
    server_write_iv: Vec<u8>,
}

impl fmt::Display for EncryptionKeys {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = format!("EncryptionKeys:\n");

        out += format!("- master_secret: {:?}\n", self.master_secret).as_str();
        out += format!("- client_mackey: {:?}\n", self.client_mac_key).as_str();
        out += format!("- server_mackey: {:?}\n", self.server_mac_key).as_str();
        out += format!("- client_write_key: {:?}\n", self.client_write_key).as_str();
        out += format!("- server_write_key: {:?}\n", self.server_write_key).as_str();
        out += format!("- client_write_iv: {:?}\n", self.client_write_iv).as_str();
        out += format!("- server_write_iv: {:?}\n", self.server_write_iv).as_str();

        write!(f, "{}", out)
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
    _public_key: &[u8],
    _private_key: &[u8],
    curve: NamedCurve,
) -> Result<Vec<u8>, Error> {
    match curve {
        //TODO: NamedCurve::X25519 => curve25519.X25519(privateKey, publicKey)
        //TODO: NamedCurve::P256 => ellipticCurvePreMasterSecret(publicKey, privateKey, elliptic.P256(), elliptic.P256())
        //TODO: NamedCurve::P384 => ellipticCurvePreMasterSecret(publicKey, privateKey, elliptic.P384(), elliptic.P384())
        _ => Err(ERR_INVALID_NAMED_CURVE.clone()),
    }
}

/*
TODO:
fn ellipticCurvePreMasterSecret(publicKey:&[u8], privateKey: &[u8], c1:, c2 elliptic.Curve) ([]byte, error) {
    x, y := elliptic.Unmarshal(c1, publicKey)
    if x == nil || y == nil {
        return nil, errInvalidNamedCurve
    }

    result, _ := c2.ScalarMult(x, y, privateKey)
    preMasterSecret := make([]byte, (c2.Params().BitSize+7)>>3)
    resultBytes := result.Bytes()
    copy(preMasterSecret[len(preMasterSecret)-len(resultBytes):], resultBytes)
    return preMasterSecret, nil
}
*/

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
/*
pub(crate) fn prfPHash<H: Hasher>(secret:&[u8], seed: &[u8], requestedLength: usize, h: H) -> Result<Vec<u8>, Error> {
    hmacSHA256 := func(key, data []byte) ([]byte, error) {
        mac := hmac.New(h, key)
        if _, err := mac.Write(data); err != nil {
            return nil, err
        }
        return mac.Sum(nil), nil
    }

    var err error
    lastRound := seed
    out := []byte{}

    iterations := int(math.Ceil(float64(requestedLength) / float64(h().Size())))
    for i := 0; i < iterations; i++ {
        lastRound, err = hmacSHA256(secret, lastRound)
        if err != nil {
            return nil, err
        }
        withSecret, err := hmacSHA256(secret, append(lastRound, seed...))
        if err != nil {
            return nil, err
        }
        out = append(out, withSecret...)
    }

    return out[:requestedLength], nil
}
*/
