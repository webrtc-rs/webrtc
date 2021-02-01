#[cfg(test)]
mod test {
    use crate::{context, error::Error, key_derivation, protection_profile::ProtectionProfile};

    #[test]
    fn test_valid_session_keys() -> Result<(), Error> {
        // Key Derivation Test Vectors from https://tools.ietf.org/html/rfc3711#appendix-B.3
        let master_key = vec![
            0xE1, 0xF9, 0x7A, 0x0D, 0x3E, 0x01, 0x8B, 0xE0, 0xD6, 0x4F, 0xA3, 0x2C, 0x06, 0xDE,
            0x41, 0x39,
        ];
        let master_salt = vec![
            0x0E, 0xC6, 0x75, 0xAD, 0x49, 0x8A, 0xFE, 0xEB, 0xB6, 0x96, 0x0B, 0x3A, 0xAB, 0xE6,
        ];

        let expected_session_key = vec![
            0xC6, 0x1E, 0x7A, 0x93, 0x74, 0x4F, 0x39, 0xEE, 0x10, 0x73, 0x4A, 0xFE, 0x3F, 0xF7,
            0xA0, 0x87,
        ];
        let expected_session_salt = vec![
            0x30, 0xCB, 0xBC, 0x08, 0x86, 0x3D, 0x8C, 0x85, 0xD4, 0x9D, 0xB3, 0x4A, 0x9A, 0xE1,
        ];
        let expected_session_auth_tag = vec![
            0xCE, 0xBE, 0x32, 0x1F, 0x6F, 0xF7, 0x71, 0x6B, 0x6F, 0xD4, 0xAB, 0x49, 0xAF, 0x25,
            0x6A, 0x15, 0x6D, 0x38, 0xBA, 0xA4,
        ];

        let session_key = key_derivation::aes_cm_key_derivation(
            context::LABEL_SRTP_ENCRYPTION,
            &master_key,
            &master_salt,
            0,
            master_key.len(),
        )?;
        assert_eq!(
            session_key, expected_session_key,
            "Session Key:\n{:?} \ndoes not match expected:\n{:?}\nMaster Key:\n{:?}\nMaster Salt:\n{:?}\n",
            session_key, expected_session_key, master_key, master_salt,
        );

        let session_salt = key_derivation::aes_cm_key_derivation(
            context::LABEL_SRTP_SALT,
            &master_key,
            &master_salt,
            0,
            master_salt.len(),
        )?;
        assert_eq!(
            session_salt, expected_session_salt,
            "Session Salt {:?} does not match expected {:?}",
            session_salt, expected_session_salt
        );

        let auth_key_len = ProtectionProfile::AES128CMHMACSHA1_80.auth_key_len()?;

        let session_auth_tag = key_derivation::aes_cm_key_derivation(
            context::LABEL_SRTP_AUTHENTICATION_TAG,
            &master_key,
            &master_salt,
            0,
            auth_key_len,
        )?;
        assert_eq!(
            session_auth_tag, expected_session_auth_tag,
            "Session Auth Tag {:?} does not match expected {:?}",
            session_auth_tag, expected_session_auth_tag,
        );

        Ok(())
    }

    // This test asserts that calling aesCmKeyDerivation with a non-zero indexOverKdr fails
    // Currently this isn't supported, but the API makes sure we can add this in the future
    #[test]
    fn test_index_over_kdr() -> Result<(), Error> {
        let result = key_derivation::aes_cm_key_derivation(
            context::LABEL_SRTP_AUTHENTICATION_TAG,
            &[],
            &[],
            1,
            0,
        );
        assert!(result.is_err());

        Ok(())
    }
}
