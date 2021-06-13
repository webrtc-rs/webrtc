use crate::error::Error;
use crate::ice::ice_credential_type::ICECredentialType;

/// ICEServer describes a single STUN and TURN server that can be used by
/// the ICEAgent to establish a connection with a peer.
#[derive(Default, Debug, Clone)]
pub struct ICEServer {
    pub urls: Vec<String>,
    pub username: String,
    pub credential: String,
    pub credential_type: ICECredentialType,
}

impl ICEServer {
    pub(crate) fn parse_url(&self, url_str: &str) -> Result<ice::url::Url, Error> {
        Ok(ice::url::Url::parse_url(url_str)?)
    }

    pub(crate) fn validate(&self) -> Result<(), Error> {
        self.urls()?;
        Ok(())
    }

    pub(crate) fn urls(&self) -> Result<Vec<ice::url::Url>, Error> {
        let mut urls = vec![];

        for url_str in &self.urls {
            let mut url = self.parse_url(url_str)?;
            if url.scheme == ice::url::SchemeType::Turn || url.scheme == ice::url::SchemeType::Turns
            {
                // https://www.w3.org/TR/webrtc/#set-the-configuration (step #11.3.2)
                if self.username.is_empty() || self.credential.is_empty() {
                    return Err(Error::ErrNoTurnCredentials);
                }
                url.username = self.username.clone();

                match self.credential_type {
                    ICECredentialType::Password => {
                        // https://www.w3.org/TR/webrtc/#set-the-configuration (step #11.3.3)
                        url.password = self.credential.clone();
                    }
                    ICECredentialType::Oauth => {
                        // https://www.w3.org/TR/webrtc/#set-the-configuration (step #11.3.4)
                        /*if _, ok: = s.Credential.(OAuthCredential); !ok {
                                return nil,
                                &rtcerr.InvalidAccessError{Err: ErrTurnCredentials
                            }
                        }*/
                    }
                    _ => return Err(Error::ErrTurnCredentials),
                };
            }

            urls.push(url);
        }

        Ok(urls)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_ice_server_validate_success() {
        let tests = vec![
            (
                ICEServer {
                    urls: vec!["turn:192.158.29.39?transport=udp".to_owned()],
                    username: "unittest".to_owned(),
                    credential: "placeholder".to_owned(),
                    credential_type: ICECredentialType::Password,
                },
                true,
            ),
            (
                ICEServer {
                    urls: vec!["turn:[2001:db8:1234:5678::1]?transport=udp".to_owned()],
                    username: "unittest".to_owned(),
                    credential: "placeholder".to_owned(),
                    credential_type: ICECredentialType::Password,
                },
                true,
            ),
            /*TODO:(ICEServer{
                URLs:     []string{"turn:192.158.29.39?transport=udp"},
                Username: "unittest".to_owned(),
                Credential: OAuthCredential{
                    MACKey:      "WmtzanB3ZW9peFhtdm42NzUzNG0=",
                    AccessToken: "AAwg3kPHWPfvk9bDFL936wYvkoctMADzQ5VhNDgeMR3+ZlZ35byg972fW8QjpEl7bx91YLBPFsIhsxloWcXPhA==",
                },
                CredentialType: ICECredentialTypeOauth,
            }, true),*/
        ];

        for (ice_server, expected_validate) in tests {
            let result = ice_server.urls();
            assert_eq!(result.is_ok(), expected_validate);
        }
    }

    #[test]
    fn test_ice_server_validate_failure() {
        let tests = vec![
            (
                ICEServer {
                    urls: vec!["turn:192.158.29.39?transport=udp".to_owned()],
                    ..Default::default()
                },
                Error::ErrNoTurnCredentials.to_string(),
            ),
            (
                ICEServer {
                    urls: vec!["turn:192.158.29.39?transport=udp".to_owned()],
                    username: "unittest".to_owned(),
                    credential: String::new(),
                    credential_type: ICECredentialType::Password,
                },
                Error::ErrNoTurnCredentials.to_string(),
            ),
            (
                ICEServer {
                    urls: vec!["turn:192.158.29.39?transport=udp".to_owned()],
                    username: "unittest".to_owned(),
                    credential: String::new(),
                    credential_type: ICECredentialType::Oauth,
                },
                Error::ErrNoTurnCredentials.to_string(),
            ),
            (
                ICEServer {
                    urls: vec!["turn:192.158.29.39?transport=udp".to_owned()],
                    username: "unittest".to_owned(),
                    credential: String::new(),
                    credential_type: ICECredentialType::Unspecified,
                },
                Error::ErrNoTurnCredentials.to_string(),
            ),
            (
                ICEServer {
                    urls: vec!["stun:google.de?transport=udp".to_owned()],
                    username: "unittest".to_owned(),
                    credential: String::new(),
                    credential_type: ICECredentialType::Oauth,
                },
                //Error::ErrUtilError.to_string(),
                "UtilError: queries not supported in stun address".to_owned(),
            ),
        ];

        for (ice_server, expected_err) in tests {
            if let Err(err) = ice_server.urls() {
                assert_eq!(err.to_string(), expected_err, "{:?}", ice_server);
            } else {
                assert!(false, "expected error, but got ok");
            }
        }
    }
}
