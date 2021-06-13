use serde::{Deserialize, Serialize};
use std::fmt;

/// DtlsRole indicates the role of the DTLS transport.
#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub enum DTLSRole {
    Unspecified = 0,

    /// DTLSRoleAuto defines the DTLS role is determined based on
    /// the resolved ICE role: the ICE controlled role acts as the DTLS
    /// client and the ICE controlling role acts as the DTLS server.
    Auto = 1,

    /// DTLSRoleClient defines the DTLS client role.
    Client = 2,

    /// DTLSRoleServer defines the DTLS server role.
    Server = 3,
}

/// https://tools.ietf.org/html/rfc5763
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
            DTLSRole::Auto => write!(f, "Auto"),
            DTLSRole::Client => write!(f, "Client"),
            DTLSRole::Server => write!(f, "Server"),
            _ => write!(f, "{}", crate::UNSPECIFIED_STR),
        }
    }
}

/*TODO:
// Iterate a SessionDescription from a remote to determine if an explicit
// role can been determined from it. The decision is made from the first role we we parse.
// If no role can be found we return DTLSRoleAuto
func dtlsRoleFromRemoteSDP(sessionDescription *sdp.SessionDescription) DTLSRole {
    if sessionDescription == nil {
        return DTLSRoleAuto
    }

    for _, mediaSection := range sessionDescription.MediaDescriptions {
        for _, attribute := range mediaSection.Attributes {
            if attribute.Key == "setup" {
                switch attribute.Value {
                case sdp.ConnectionRoleActive.String():
                    return DTLSRoleClient
                case sdp.ConnectionRolePassive.String():
                    return DTLSRoleServer
                default:
                    return DTLSRoleAuto
                }
            }
        }
    }

    return DTLSRoleAuto
}

func connectionRoleFromDtlsRole(d DTLSRole) sdp.ConnectionRole {
    switch d {
    case DTLSRoleClient:
        return sdp.ConnectionRoleActive
    case DTLSRoleServer:
        return sdp.ConnectionRolePassive
    case DTLSRoleAuto:
        return sdp.ConnectionRoleActpass
    default:
        return sdp.ConnectionRole(0)
    }
}*/

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_dtls_role_string() {
        let tests = vec![
            (DTLSRole::Unspecified, "Unspecified"),
            (DTLSRole::Auto, "Auto"),
            (DTLSRole::Client, "Client"),
            (DTLSRole::Server, "Server"),
        ];

        for (role, expected_string) in tests {
            assert_eq!(expected_string, role.to_string(),)
        }
    }

    /*TODO: func TestDTLSRoleFromRemoteSDP(t *testing.T) {
        parseSDP := func(raw string) *sdp.SessionDescription {
            parsed := &sdp.SessionDescription{}
            if err := parsed.Unmarshal([]byte(raw)); err != nil {
                panic(err)
            }
            return parsed
        }

        const noMedia = `v=0
    o=- 4596489990601351948 2 IN IP4 127.0.0.1
    s=-
    t=0 0
    `

        const mediaNoSetup = `v=0
    o=- 4596489990601351948 2 IN IP4 127.0.0.1
    s=-
    t=0 0
    m=application 47299 DTLS/SCTP 5000
    c=IN IP4 192.168.20.129
    `

        const mediaSetupDeclared = `v=0
    o=- 4596489990601351948 2 IN IP4 127.0.0.1
    s=-
    t=0 0
    m=application 47299 DTLS/SCTP 5000
    c=IN IP4 192.168.20.129
    a=setup:%s
    `

        testCases := []struct {
            test               string
            sessionDescription *sdp.SessionDescription
            expectedRole       DTLSRole
        }{
            {"nil SessionDescription", nil, DTLSRoleAuto},
            {"No MediaDescriptions", parseSDP(noMedia), DTLSRoleAuto},
            {"MediaDescription, no setup", parseSDP(mediaNoSetup), DTLSRoleAuto},
            {"MediaDescription, setup:actpass", parseSDP(fmt.Sprintf(mediaSetupDeclared, "actpass")), DTLSRoleAuto},
            {"MediaDescription, setup:passive", parseSDP(fmt.Sprintf(mediaSetupDeclared, "passive")), DTLSRoleServer},
            {"MediaDescription, setup:active", parseSDP(fmt.Sprintf(mediaSetupDeclared, "active")), DTLSRoleClient},
        }
        for _, testCase := range testCases {
            assert.Equal(t,
                testCase.expectedRole,
                dtlsRoleFromRemoteSDP(testCase.sessionDescription),
                "TestDTLSRoleFromSDP (%s)", testCase.test,
            )
        }
    }*/
}
