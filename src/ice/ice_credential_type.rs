/// ICECredentialType indicates the type of credentials used to connect to
/// an ICE server.
pub enum ICECredentialType {
    /// ICECredential::Password describes username and password based
    /// credentials as described in https://tools.ietf.org/html/rfc5389.
    Password,
    //Not supported in WebRTC 1.0 spec
    // ICECredential::Oauth describes token based credential as described
    // in https://tools.ietf.org/html/rfc7635.
    //Oauth,
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
