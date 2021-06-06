/// ICECredentialType indicates the type of credentials used to connect to
/// an ICE server.
#[derive(Debug, Copy, Clone)]
pub enum ICECredentialType {
    /// ICECredential::Password describes username and password based
    /// credentials as described in https://tools.ietf.org/html/rfc5389.
    Password = 0,

    /// ICECredential::Oauth describes token based credential as described
    /// in https://tools.ietf.org/html/rfc7635.
    /// Not supported in WebRTC 1.0 spec
    Oauth = 1,
}

impl Default for ICECredentialType {
    fn default() -> Self {
        ICECredentialType::Password
    }
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
