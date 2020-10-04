use util::Error;

use super::context::*;
use super::protection_profile::*;

const LABEL_EXTRACTOR_DTLS_SRTP: &'static str = "EXTRACTOR-dtls_srtp";

// KeyingMaterialExporter allows package SRTP to extract keying material
pub trait KeyingMaterialExporter {
    fn export_keying_material(
        &self,
        label: String,
        context: &[u8],
        length: usize,
    ) -> Result<Vec<u8>, Error>;
}

// SessionKeys bundles the keys required to setup an SRTP session
pub struct SessionKeys {
    pub local_master_key: Vec<u8>,
    pub local_master_salt: Vec<u8>,
    pub remote_master_key: Vec<u8>,
    pub remote_master_salt: Vec<u8>,
}

// Config is used to configure a session.
// You can provide either a KeyingMaterialExporter to export keys
// or directly pass the keys themselves.
// After a Config is passed to a session it must not be modified.
pub struct Config {
    pub keys: SessionKeys,
    pub profile: ProtectionProfile,
    //LoggerFactory: logging.LoggerFactory
}

impl Config {
    // ExtractSessionKeysFromDTLS allows setting the Config SessionKeys by
    // extracting them from DTLS. This behavior is defined in RFC5764:
    // https://tools.ietf.org/html/rfc5764
    pub fn extract_session_keys_from_dtls(
        &mut self,
        exporter: impl KeyingMaterialExporter,
        is_client: bool,
    ) -> Result<(), Error> {
        let keying_material = exporter.export_keying_material(
            LABEL_EXTRACTOR_DTLS_SRTP.to_string(),
            &vec![],
            (KEY_LEN * 2) + (SALT_LEN * 2),
        )?;

        let mut offset = 0;
        let client_write_key = keying_material[offset..offset + KEY_LEN].to_vec();
        offset += KEY_LEN;

        let server_write_key = keying_material[offset..offset + KEY_LEN].to_vec();
        offset += KEY_LEN;

        let client_write_salt = keying_material[offset..offset + SALT_LEN].to_vec();
        offset += SALT_LEN;

        let server_write_salt = keying_material[offset..offset + SALT_LEN].to_vec();

        if is_client {
            self.keys.local_master_key = client_write_key;
            self.keys.local_master_salt = client_write_salt;
            self.keys.remote_master_key = server_write_key;
            self.keys.remote_master_salt = server_write_salt;
        } else {
            self.keys.local_master_key = server_write_key;
            self.keys.local_master_salt = server_write_salt;
            self.keys.remote_master_key = client_write_key;
            self.keys.remote_master_salt = client_write_salt;
        }

        Ok(())
    }
}
