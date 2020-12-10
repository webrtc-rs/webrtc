#[cfg(test)]
mod signature_hash_algorithm_test;

use std::fmt;

use crate::crypto::*;
use crate::errors::*;

use util::Error;

// HashAlgorithm is used to indicate the hash algorithm used
// https://www.iana.org/assignments/tls-parameters/tls-parameters.xhtml#tls-parameters-18
// Supported hash hash algorithms
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum HashAlgorithm {
    MD2 = 0,  // Blacklisted
    MD5 = 1,  // Blacklisted
    SHA1 = 2, // Blacklisted
    SHA224 = 3,
    SHA256 = 4,
    SHA384 = 5,
    SHA512 = 6,
    Ed25519 = 8,
    Unsupported,
}

impl From<u8> for HashAlgorithm {
    fn from(val: u8) -> Self {
        match val {
            0 => HashAlgorithm::MD2,
            1 => HashAlgorithm::MD5,
            2 => HashAlgorithm::SHA1,
            3 => HashAlgorithm::SHA224,
            4 => HashAlgorithm::SHA256,
            5 => HashAlgorithm::SHA384,
            6 => HashAlgorithm::SHA512,
            8 => HashAlgorithm::Ed25519,
            _ => HashAlgorithm::Unsupported,
        }
    }
}

impl fmt::Display for HashAlgorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            HashAlgorithm::MD2 => write!(f, "md2"),
            HashAlgorithm::MD5 => write!(f, "md5"), // [RFC3279]
            HashAlgorithm::SHA1 => write!(f, "sha-1"), // [RFC3279]
            HashAlgorithm::SHA224 => write!(f, "sha-224"), // [RFC4055]
            HashAlgorithm::SHA256 => write!(f, "sha-256"), // [RFC4055]
            HashAlgorithm::SHA384 => write!(f, "sha-384"), // [RFC4055]
            HashAlgorithm::SHA512 => write!(f, "sha-512"), // [RFC4055]
            HashAlgorithm::Ed25519 => write!(f, "null"), // [RFC4055]
            _ => write!(f, "unknown or unsupported hash algorithm"),
        }
    }
}

impl HashAlgorithm {
    pub(crate) fn insecure(&self) -> bool {
        matches!(
            *self,
            HashAlgorithm::MD2 | HashAlgorithm::MD5 | HashAlgorithm::SHA1
        )
    }

    pub(crate) fn invalid(&self) -> bool {
        matches!(*self, HashAlgorithm::MD2)
    }
}

// https://www.iana.org/assignments/tls-parameters/tls-parameters.xhtml#tls-parameters-16
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum SignatureAlgorithm {
    RSA = 1,
    ECDSA = 3,
    Ed25519 = 7,
    Unsupported,
}

impl From<u8> for SignatureAlgorithm {
    fn from(val: u8) -> Self {
        match val {
            1 => SignatureAlgorithm::RSA,
            3 => SignatureAlgorithm::ECDSA,
            7 => SignatureAlgorithm::Ed25519,
            _ => SignatureAlgorithm::Unsupported,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct SignatureHashAlgorithm {
    pub hash: HashAlgorithm,
    pub signature: SignatureAlgorithm,
}

impl SignatureHashAlgorithm {
    // is_compatible checks that given private key is compatible with the signature scheme.
    pub(crate) fn is_compatible(&self, private_key: &CryptoPrivateKey) -> bool {
        match &private_key.kind {
            CryptoPrivateKeyKind::ED25519(_) => self.signature == SignatureAlgorithm::Ed25519,
            CryptoPrivateKeyKind::ECDSA256(_) => self.signature == SignatureAlgorithm::ECDSA,
            CryptoPrivateKeyKind::RSA256(_) => self.signature == SignatureAlgorithm::RSA,
        }
    }
}

pub(crate) fn default_signature_schemes() -> Vec<SignatureHashAlgorithm> {
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
        SignatureHashAlgorithm {
            hash: HashAlgorithm::Ed25519,
            signature: SignatureAlgorithm::Ed25519,
        },
    ]
}

// select Signature Scheme returns most preferred and compatible scheme.
pub(crate) fn select_signature_scheme(
    sigs: &[SignatureHashAlgorithm],
    private_key: &CryptoPrivateKey,
) -> Result<SignatureHashAlgorithm, Error> {
    for ss in sigs {
        if ss.is_compatible(private_key) {
            return Ok(*ss);
        }
    }

    Err(ERR_NO_AVAILABLE_SIGNATURE_SCHEMES.clone())
}

// SignatureScheme identifies a signature algorithm supported by TLS. See
// RFC 8446, Section 4.2.3.
#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) enum SignatureScheme {
    // RSASSA-PKCS1-v1_5 algorithms.
    PKCS1WithSHA256 = 0x0401,
    PKCS1WithSHA384 = 0x0501,
    PKCS1WithSHA512 = 0x0601,

    // RSASSA-PSS algorithms with public key OID rsaEncryption.
    PSSWithSHA256 = 0x0804,
    PSSWithSHA384 = 0x0805,
    PSSWithSHA512 = 0x0806,

    // ECDSA algorithms. Only constrained to a specific curve in TLS 1.3.
    ECDSAWithP256AndSHA256 = 0x0403,
    ECDSAWithP384AndSHA384 = 0x0503,
    ECDSAWithP521AndSHA512 = 0x0603,

    // EdDSA algorithms.
    Ed25519 = 0x0807,

    // Legacy signature and hash algorithms for TLS 1.2.
    PKCS1WithSHA1 = 0x0201,
    ECDSAWithSHA1 = 0x0203,
}

// parse_signature_schemes translates []tls.SignatureScheme to []signatureHashAlgorithm.
// It returns default signature scheme list if no SignatureScheme is passed.
pub(crate) fn parse_signature_schemes(
    sigs: &[u16],
    insecure_hashes: bool,
) -> Result<Vec<SignatureHashAlgorithm>, Error> {
    if sigs.is_empty() {
        return Ok(default_signature_schemes());
    }

    let mut out = vec![];
    for ss in sigs {
        let sig: SignatureAlgorithm = ((*ss & 0xFF) as u8).into();
        if sig == SignatureAlgorithm::Unsupported {
            return Err(ERR_INVALID_SIGNATURE_ALGORITHM.clone());
        }
        let h: HashAlgorithm = (((*ss >> 8) & 0xFF) as u8).into();
        if h == HashAlgorithm::Unsupported || h.invalid() {
            return Err(ERR_INVALID_HASH_ALGORITHM.clone());
        }
        if h.insecure() && !insecure_hashes {
            continue;
        }
        out.push(SignatureHashAlgorithm {
            hash: h,
            signature: sig,
        })
    }

    if out.is_empty() {
        Err(ERR_NO_AVAILABLE_SIGNATURE_SCHEMES.clone())
    } else {
        Ok(out)
    }
}
