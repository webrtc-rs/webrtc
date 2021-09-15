use std::fmt;

/// ICECredentialType indicates the type of credentials used to connect to
/// an ICE server.
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum ICECredentialType {
    Unspecified,

    /// ICECredential::Password describes username and password based
    /// credentials as described in https://tools.ietf.org/html/rfc5389.
    Password,

    /// ICECredential::Oauth describes token based credential as described
    /// in https://tools.ietf.org/html/rfc7635.
    /// Not supported in WebRTC 1.0 spec
    Oauth,
}

impl Default for ICECredentialType {
    fn default() -> Self {
        ICECredentialType::Password
    }
}

const ICE_CREDENTIAL_TYPE_PASSWORD_STR: &str = "password";
const ICE_CREDENTIAL_TYPE_OAUTH_STR: &str = "oauth";

impl From<&str> for ICECredentialType {
    fn from(raw: &str) -> Self {
        match raw {
            ICE_CREDENTIAL_TYPE_PASSWORD_STR => ICECredentialType::Password,
            ICE_CREDENTIAL_TYPE_OAUTH_STR => ICECredentialType::Oauth,
            _ => ICECredentialType::Unspecified,
        }
    }
}

impl fmt::Display for ICECredentialType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            ICECredentialType::Password => write!(f, "{}", ICE_CREDENTIAL_TYPE_PASSWORD_STR),
            ICECredentialType::Oauth => write!(f, "{}", ICE_CREDENTIAL_TYPE_OAUTH_STR),
            _ => write!(f, "{}", crate::UNSPECIFIED_STR),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_new_ice_credential_type() {
        let tests = vec![
            ("Unspecified", ICECredentialType::Unspecified),
            ("password", ICECredentialType::Password),
            ("oauth", ICECredentialType::Oauth),
        ];

        for (ct_str, expected_ct) in tests {
            assert_eq!(expected_ct, ICECredentialType::from(ct_str));
        }
    }

    #[test]
    fn test_ice_credential_type_string() {
        let tests = vec![
            (ICECredentialType::Unspecified, "Unspecified"),
            (ICECredentialType::Password, "password"),
            (ICECredentialType::Oauth, "oauth"),
        ];

        for (ct, expected_string) in tests {
            assert_eq!(expected_string, ct.to_string());
        }
    }
}
