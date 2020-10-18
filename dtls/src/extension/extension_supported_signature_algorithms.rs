use super::*;
use crate::signature_hash_algorithm::*;

const EXTENSION_SUPPORTED_SIGNATURE_ALGORITHMS_HEADER_SIZE: usize = 6;

// https://tools.ietf.org/html/rfc5246#section-7.4.1.4.1
#[derive(Clone, Debug, PartialEq)]
pub struct ExtensionSupportedSignatureAlgorithms {
    signature_hash_algorithms: Vec<SignatureHashAlgorithm>,
}

impl ExtensionSupportedSignatureAlgorithms {
    pub fn extension_value() -> ExtensionValue {
        ExtensionValue::SupportedSignatureAlgorithms
    }
}
