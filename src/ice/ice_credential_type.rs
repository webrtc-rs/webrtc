/// ICECredential indicates the type of credentials used to connect to
/// an ICE server.
pub enum ICECredential {
    /// ICECredential::Password describes username and password based
    /// credentials as described in https://tools.ietf.org/html/rfc5389.
    Password(String),

    /// ICECredential::Oauth describes token based credential as described
    /// in https://tools.ietf.org/html/rfc7635.
    Oauth(OAuthCredential),
}

/// OAuthCredential represents OAuth credential information which is used by
/// the STUN/TURN client to connect to an ICE server as defined in
/// https://tools.ietf.org/html/rfc7635. Note that the kid parameter is not
/// located in OAuthCredential, but in ICEServer's username member.
pub struct OAuthCredential {
    /// mackey is a base64-url encoded format. It is used in STUN message
    /// integrity hash calculation.
    mac_key: String,

    /// access_token is a base64-encoded format. This is an encrypted
    /// self-contained token that is opaque to the application.
    access_token: String,
}

/*
// This is done this way because of a linter.
const (
    iceCredentialTypePasswordStr = "password"
    iceCredentialTypeOauthStr    = "oauth"
)

func newICECredentialType(raw string) ICECredentialType {
    switch raw {
    case iceCredentialTypePasswordStr:
        return ICECredentialTypePassword
    case iceCredentialTypeOauthStr:
        return ICECredentialTypeOauth
    default:
        return ICECredentialType(Unknown)
    }
}

func (t ICECredentialType) String() string {
    switch t {
    case ICECredentialTypePassword:
        return iceCredentialTypePasswordStr
    case ICECredentialTypeOauth:
        return iceCredentialTypeOauthStr
    default:
        return ErrUnknownType.Error()
    }
}
*/
