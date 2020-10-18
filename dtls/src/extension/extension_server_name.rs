use super::*;

const EXTENSION_SERVER_NAME_TYPE_DNSHOST_NAME: u8 = 0;

#[derive(Clone, Debug, PartialEq)]
pub struct ExtensionServerName {
    server_name: String,
}

impl ExtensionServerName {
    pub fn extension_value() -> ExtensionValue {
        ExtensionValue::ServerName
    }
}
