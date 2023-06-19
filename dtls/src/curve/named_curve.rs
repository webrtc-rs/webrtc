use rand_core::OsRng; // requires 'getrandom' feature

use crate::error::*;

// https://www.iana.org/assignments/tls-parameters/tls-parameters.xml#tls-parameters-8
#[repr(u16)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum NamedCurve {
    Unsupported = 0x0000,
    P256 = 0x0017,
    P384 = 0x0018,
    X25519 = 0x001d,
}

impl From<u16> for NamedCurve {
    fn from(val: u16) -> Self {
        match val {
            0x0017 => NamedCurve::P256,
            0x0018 => NamedCurve::P384,
            0x001d => NamedCurve::X25519,
            _ => NamedCurve::Unsupported,
        }
    }
}

pub(crate) enum NamedCurvePrivateKey {
    EphemeralSecretP256(p256::ecdh::EphemeralSecret),
    EphemeralSecretP384(p384::ecdh::EphemeralSecret),
    StaticSecretX25519(x25519_dalek::StaticSecret),
}

pub struct NamedCurveKeypair {
    pub(crate) curve: NamedCurve,
    pub(crate) public_key: Vec<u8>,
    pub(crate) private_key: NamedCurvePrivateKey,
}

fn elliptic_curve_keypair(curve: NamedCurve) -> Result<NamedCurveKeypair> {
    let (public_key, private_key) = match curve {
        NamedCurve::P256 => {
            let secret_key = p256::ecdh::EphemeralSecret::random(&mut OsRng);
            let public_key = p256::EncodedPoint::from(secret_key.public_key());
            (
                public_key.as_bytes().to_vec(),
                NamedCurvePrivateKey::EphemeralSecretP256(secret_key),
            )
        }
        NamedCurve::P384 => {
            let secret_key = p384::ecdh::EphemeralSecret::random(&mut OsRng);
            let public_key = p384::EncodedPoint::from(secret_key.public_key());
            (
                public_key.as_bytes().to_vec(),
                NamedCurvePrivateKey::EphemeralSecretP384(secret_key),
            )
        }
        NamedCurve::X25519 => {
            let secret_key = x25519_dalek::StaticSecret::random_from_rng(OsRng);
            let public_key = x25519_dalek::PublicKey::from(&secret_key);
            (
                public_key.as_bytes().to_vec(),
                NamedCurvePrivateKey::StaticSecretX25519(secret_key),
            )
        }
        _ => return Err(Error::ErrInvalidNamedCurve),
    };

    Ok(NamedCurveKeypair {
        curve,
        public_key,
        private_key,
    })
}

impl NamedCurve {
    pub fn generate_keypair(&self) -> Result<NamedCurveKeypair> {
        match *self {
            NamedCurve::X25519 => elliptic_curve_keypair(NamedCurve::X25519),
            NamedCurve::P256 => elliptic_curve_keypair(NamedCurve::P256),
            NamedCurve::P384 => elliptic_curve_keypair(NamedCurve::P384),
            _ => Err(Error::ErrInvalidNamedCurve),
        }
    }
}
