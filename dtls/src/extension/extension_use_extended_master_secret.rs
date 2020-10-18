use super::*;

const EXTENSION_USE_EXTENDED_MASTER_SECRET_HEADER_SIZE: usize = 4;

// https://tools.ietf.org/html/rfc8422
#[derive(Clone, Debug, PartialEq)]
pub struct ExtensionUseExtendedMasterSecret {
    supported: bool,
}

impl ExtensionUseExtendedMasterSecret {
    pub fn extension_value() -> ExtensionValue {
        ExtensionValue::UseExtendedMasterSecret
    }
}
