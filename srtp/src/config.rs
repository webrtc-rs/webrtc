use util::KeyingMaterialExporter;

use crate::error::Result;
use crate::option::*;
use crate::protection_profile::*;

const LABEL_EXTRACTOR_DTLS_SRTP: &str = "EXTRACTOR-dtls_srtp";

/// SessionKeys bundles the keys required to setup an SRTP session
#[derive(Default, Debug, Clone)]
pub struct SessionKeys {
    pub local_master_key: Vec<u8>,
    pub local_master_salt: Vec<u8>,
    pub remote_master_key: Vec<u8>,
    pub remote_master_salt: Vec<u8>,
}

/// Config is used to configure a session.
/// You can provide either a KeyingMaterialExporter to export keys
/// or directly pass the keys themselves.
/// After a Config is passed to a session it must not be modified.
#[derive(Default)]
pub struct Config {
    pub keys: SessionKeys,
    pub profile: ProtectionProfile,
    //LoggerFactory: logging.LoggerFactory
    /// List of local/remote context options.
    /// ReplayProtection is enabled on remote context by default.
    /// Default replay protection window size is 64.
    pub local_rtp_options: Option<ContextOption>,
    pub remote_rtp_options: Option<ContextOption>,

    pub local_rtcp_options: Option<ContextOption>,
    pub remote_rtcp_options: Option<ContextOption>,
}

impl Config {
    /// ExtractSessionKeysFromDTLS allows setting the Config SessionKeys by
    /// extracting them from DTLS. This behavior is defined in RFC5764:
    /// <https://tools.ietf.org/html/rfc5764>
    pub async fn extract_session_keys_from_dtls(
        &mut self,
        exporter: impl KeyingMaterialExporter,
        is_client: bool,
    ) -> Result<()> {
        let key_len = self.profile.key_len();
        let salt_len = self.profile.salt_len();

        let keying_material = exporter
            .export_keying_material(
                LABEL_EXTRACTOR_DTLS_SRTP,
                &[],
                (key_len * 2) + (salt_len * 2),
            )
            .await?;

        let mut offset = 0;
        let client_write_key = keying_material[offset..offset + key_len].to_vec();
        offset += key_len;

        let server_write_key = keying_material[offset..offset + key_len].to_vec();
        offset += key_len;

        let client_write_salt = keying_material[offset..offset + salt_len].to_vec();
        offset += salt_len;

        let server_write_salt = keying_material[offset..offset + salt_len].to_vec();

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
