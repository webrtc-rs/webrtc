#[cfg(test)]
mod signature_hash_algorithm_test;

use std::fmt;

use crate::crypto::*;
use crate::error::*;

// HashAlgorithm is used to indicate the hash algorithm used
// https://www.iana.org/assignments/tls-parameters/tls-parameters.xhtml#tls-parameters-18
// Supported hash hash algorithms
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum HashAlgorithm {
    Md2 = 0,  // Blacklisted
    Md5 = 1,  // Blacklisted
    Sha1 = 2, // Blacklisted
    Sha224 = 3,
    Sha256 = 4,
    Sha384 = 5,
    Sha512 = 6,
    Ed25519 = 8,
    Unsupported,
}

impl From<u8> for HashAlgorithm {
    fn from(val: u8) -> Self {
        match val {
            0 => HashAlgorithm::Md2,
            1 => HashAlgorithm::Md5,
            2 => HashAlgorithm::Sha1,
            3 => HashAlgorithm::Sha224,
            4 => HashAlgorithm::Sha256,
            5 => HashAlgorithm::Sha384,
            6 => HashAlgorithm::Sha512,
            8 => HashAlgorithm::Ed25519,
            _ => HashAlgorithm::Unsupported,
        }
    }
}

impl fmt::Display for HashAlgorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            HashAlgorithm::Md2 => write!(f, "md2"),
            HashAlgorithm::Md5 => write!(f, "md5"), // [RFC3279]
            HashAlgorithm::Sha1 => write!(f, "sha-1"), // [RFC3279]
            HashAlgorithm::Sha224 => write!(f, "sha-224"), // [RFC4055]
            HashAlgorithm::Sha256 => write!(f, "sha-256"), // [RFC4055]
            HashAlgorithm::Sha384 => write!(f, "sha-384"), // [RFC4055]
            HashAlgorithm::Sha512 => write!(f, "sha-512"), // [RFC4055]
            HashAlgorithm::Ed25519 => write!(f, "null"), // [RFC4055]
            _ => write!(f, "unknown or unsupported hash algorithm"),
        }
    }
}

impl HashAlgorithm {
    pub(crate) fn insecure(&self) -> bool {
        matches!(
            *self,
            HashAlgorithm::Md2 | HashAlgorithm::Md5 | HashAlgorithm::Sha1
        )
    }

    pub(crate) fn invalid(&self) -> bool {
        matches!(*self, HashAlgorithm::Md2)
    }
}

// https://www.iana.org/assignments/tls-parameters/tls-parameters.xhtml#tls-parameters-16
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SignatureAlgorithm {
    Rsa = 1,
    Ecdsa = 3,
    Ed25519 = 7,
    Unsupported,
}

impl From<u8> for SignatureAlgorithm {
    fn from(val: u8) -> Self {
        match val {
            1 => SignatureAlgorithm::Rsa,
            3 => SignatureAlgorithm::Ecdsa,
            7 => SignatureAlgorithm::Ed25519,
            _ => SignatureAlgorithm::Unsupported,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct SignatureHashAlgorithm {
    pub hash: HashAlgorithm,
    pub signature: SignatureAlgorithm,
}

impl SignatureHashAlgorithm {
    // is_compatible checks that given private key is compatible with the signature scheme.
    pub(crate) fn is_compatible(&self, private_key: &CryptoPrivateKey) -> bool {
        match &private_key.kind {
            CryptoPrivateKeyKind::Ed25519(_) => self.signature == SignatureAlgorithm::Ed25519,
            CryptoPrivateKeyKind::Ecdsa256(_) => self.signature == SignatureAlgorithm::Ecdsa,
            CryptoPrivateKeyKind::Rsa256(_) => self.signature == SignatureAlgorithm::Rsa,
        }
    }
}

pub(crate) fn default_signature_schemes() -> Vec<SignatureHashAlgorithm> {
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
) -> Result<SignatureHashAlgorithm> {
    for ss in sigs {
        if ss.is_compatible(private_key) {
            return Ok(*ss);
        }
    }

    Err(Error::ErrNoAvailableSignatureSchemes)
}

// SignatureScheme identifies a signature algorithm supported by TLS. See
// RFC 8446, Section 4.2.3.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SignatureScheme {
    // RSASSA-PKCS1-v1_5 algorithms.
    Pkcs1WithSha256 = 0x0401,
    Pkcs1WithSha384 = 0x0501,
    Pkcs1WithSha512 = 0x0601,

    // RSASSA-PSS algorithms with public key OID rsaEncryption.
    PssWithSha256 = 0x0804,
    PssWithSha384 = 0x0805,
    PssWithSha512 = 0x0806,

    // ECDSA algorithms. Only constrained to a specific curve in TLS 1.3.
    EcdsaWithP256AndSha256 = 0x0403,
    EcdsaWithP384AndSha384 = 0x0503,
    EcdsaWithP521AndSha512 = 0x0603,

    // EdDSA algorithms.
    Ed25519 = 0x0807,

    // Legacy signature and hash algorithms for TLS 1.2.
    Pkcs1WithSha1 = 0x0201,
    EcdsaWithSha1 = 0x0203,
}

// parse_signature_schemes translates []tls.SignatureScheme to []signatureHashAlgorithm.
// It returns default signature scheme list if no SignatureScheme is passed.
pub(crate) fn parse_signature_schemes(
    sigs: &[u16],
    insecure_hashes: bool,
) -> Result<Vec<SignatureHashAlgorithm>> {
    if sigs.is_empty() {
        return Ok(default_signature_schemes());
    }

    let mut out = vec![];
    for ss in sigs {
        let sig: SignatureAlgorithm = ((*ss & 0xFF) as u8).into();
        if sig == SignatureAlgorithm::Unsupported {
            return Err(Error::ErrInvalidSignatureAlgorithm);
        }
        let h: HashAlgorithm = (((*ss >> 8) & 0xFF) as u8).into();
        if h == HashAlgorithm::Unsupported || h.invalid() {
            return Err(Error::ErrInvalidHashAlgorithm);
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
        Err(Error::ErrNoAvailableSignatureSchemes)
    } else {
        Ok(out)
    }
}
