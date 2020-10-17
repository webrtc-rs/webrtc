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
