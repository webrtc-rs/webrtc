use sdp::description::session::SessionDescription;
use sdp::util::ConnectionRole;

use serde::{Deserialize, Serialize};
use std::fmt;

/// DtlsRole indicates the role of the DTLS transport.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DTLSRole {
    Unspecified = 0,

    /// DTLSRoleAuto defines the DTLS role is determined based on
    /// the resolved ICE role: the ICE controlled role acts as the DTLS
    /// client and the ICE controlling role acts as the DTLS server.
    #[serde(rename = "auto")]
    Auto = 1,

    /// DTLSRoleClient defines the DTLS client role.
    #[serde(rename = "client")]
    Client = 2,

    /// DTLSRoleServer defines the DTLS server role.
    #[serde(rename = "server")]
    Server = 3,
}

/// <https://tools.ietf.org/html/rfc5763>
/// The answerer MUST use either a
/// setup attribute value of setup:active or setup:passive.  Note that
/// if the answerer uses setup:passive, then the DTLS handshake will
/// not begin until the answerer is received, which adds additional
/// latency. setup:active allows the answer and the DTLS handshake to
/// occur in parallel.  Thus, setup:active is RECOMMENDED.
pub(crate) const DEFAULT_DTLS_ROLE_ANSWER: DTLSRole = DTLSRole::Client;

/// The endpoint that is the offerer MUST use the setup attribute
/// value of setup:actpass and be prepared to receive a client_hello
/// before it receives the answer.
pub(crate) const DEFAULT_DTLS_ROLE_OFFER: DTLSRole = DTLSRole::Auto;

impl Default for DTLSRole {
    fn default() -> Self {
        DTLSRole::Unspecified
    }
}

impl fmt::Display for DTLSRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            DTLSRole::Auto => write!(f, "auto"),
            DTLSRole::Client => write!(f, "client"),
            DTLSRole::Server => write!(f, "server"),
            _ => write!(f, "{}", crate::UNSPECIFIED_STR),
        }
    }
}

/// Iterate a SessionDescription from a remote to determine if an explicit
/// role can been determined from it. The decision is made from the first role we we parse.
/// If no role can be found we return DTLSRoleAuto
impl From<&SessionDescription> for DTLSRole {
    fn from(session_description: &SessionDescription) -> Self {
        for media_section in &session_description.media_descriptions {
            for attribute in &media_section.attributes {
                if attribute.key == "setup" {
                    if let Some(value) = &attribute.value {
                        match value.as_str() {
                            "active" => return DTLSRole::Client,
                            "passive" => return DTLSRole::Server,
                            _ => return DTLSRole::Auto,
                        };
                    } else {
                        return DTLSRole::Auto;
                    }
                }
            }
        }

        DTLSRole::Auto
    }
}

impl DTLSRole {
    pub(crate) fn to_connection_role(self) -> ConnectionRole {
        match self {
            DTLSRole::Client => ConnectionRole::Active,
            DTLSRole::Server => ConnectionRole::Passive,
            DTLSRole::Auto => ConnectionRole::Actpass,
            _ => ConnectionRole::Unspecified,
        }
    }
}

#[cfg(test)]
mod test {
    use crate::error::Result;

    use super::*;

    use std::io::Cursor;

    #[test]
    fn test_dtls_role_string() {
        let tests = vec![
            (DTLSRole::Unspecified, "Unspecified"),
            (DTLSRole::Auto, "auto"),
            (DTLSRole::Client, "client"),
            (DTLSRole::Server, "server"),
        ];

        for (role, expected_string) in tests {
            assert_eq!(role.to_string(), expected_string)
        }
    }

    #[test]
    fn test_dtls_role_from_remote_sdp() -> Result<()> {
        const NO_MEDIA: &str = "v=0
o=- 4596489990601351948 2 IN IP4 127.0.0.1
s=-
t=0 0
";

        const MEDIA_NO_SETUP: &str = "v=0
o=- 4596489990601351948 2 IN IP4 127.0.0.1
s=-
t=0 0
m=application 47299 DTLS/SCTP 5000
c=IN IP4 192.168.20.129
";

        const MEDIA_SETUP_DECLARED: &str = "v=0
o=- 4596489990601351948 2 IN IP4 127.0.0.1
s=-
t=0 0
m=application 47299 DTLS/SCTP 5000
c=IN IP4 192.168.20.129
a=setup:";

        let tests = vec![
            ("No MediaDescriptions", NO_MEDIA.to_owned(), DTLSRole::Auto),
            (
                "MediaDescription, no setup",
                MEDIA_NO_SETUP.to_owned(),
                DTLSRole::Auto,
            ),
            (
                "MediaDescription, setup:actpass",
                format!("{}{}\n", MEDIA_SETUP_DECLARED, "actpass"),
                DTLSRole::Auto,
            ),
            (
                "MediaDescription, setup:passive",
                format!("{}{}\n", MEDIA_SETUP_DECLARED, "passive"),
                DTLSRole::Server,
            ),
            (
                "MediaDescription, setup:active",
                format!("{}{}\n", MEDIA_SETUP_DECLARED, "active"),
                DTLSRole::Client,
            ),
        ];

        for (name, session_description_str, expected_role) in tests {
            let mut reader = Cursor::new(session_description_str.as_bytes());
            let session_description = SessionDescription::unmarshal(&mut reader)?;
            assert_eq!(
                DTLSRole::from(&session_description),
                expected_role,
                "{name} failed"
            );
        }

        Ok(())
    }
}
