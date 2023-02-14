use crate::error::{Error, Result};
use crate::ice_transport::ice_credential_type::RTCIceCredentialType;

/// ICEServer describes a single STUN and TURN server that can be used by
/// the ICEAgent to establish a connection with a peer.
#[derive(Default, Debug, Clone)]
pub struct RTCIceServer {
    pub urls: Vec<String>,
    pub username: String,
    pub credential: String,
    pub credential_type: RTCIceCredentialType,
}

impl RTCIceServer {
    pub(crate) fn parse_url(&self, url_str: &str) -> Result<ice::url::Url> {
        Ok(ice::url::Url::parse_url(url_str)?)
    }

    pub(crate) fn validate(&self) -> Result<()> {
        self.urls()?;
        Ok(())
    }

    pub(crate) fn urls(&self) -> Result<Vec<ice::url::Url>> {
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
                    RTCIceCredentialType::Password => {
                        // https://www.w3.org/TR/webrtc/#set-the-configuration (step #11.3.3)
                        url.password = self.credential.clone();
                    }
                    RTCIceCredentialType::Oauth => {
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
                RTCIceServer {
                    urls: vec!["turn:192.158.29.39?transport=udp".to_owned()],
                    username: "unittest".to_owned(),
                    credential: "placeholder".to_owned(),
                    credential_type: RTCIceCredentialType::Password,
                },
                true,
            ),
            (
                RTCIceServer {
                    urls: vec!["turn:[2001:db8:1234:5678::1]?transport=udp".to_owned()],
                    username: "unittest".to_owned(),
                    credential: "placeholder".to_owned(),
                    credential_type: RTCIceCredentialType::Password,
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
                RTCIceServer {
                    urls: vec!["turn:192.158.29.39?transport=udp".to_owned()],
                    ..Default::default()
                },
                Error::ErrNoTurnCredentials,
            ),
            (
                RTCIceServer {
                    urls: vec!["turn:192.158.29.39?transport=udp".to_owned()],
                    username: "unittest".to_owned(),
                    credential: String::new(),
                    credential_type: RTCIceCredentialType::Password,
                },
                Error::ErrNoTurnCredentials,
            ),
            (
                RTCIceServer {
                    urls: vec!["turn:192.158.29.39?transport=udp".to_owned()],
                    username: "unittest".to_owned(),
                    credential: String::new(),
                    credential_type: RTCIceCredentialType::Oauth,
                },
                Error::ErrNoTurnCredentials,
            ),
            (
                RTCIceServer {
                    urls: vec!["turn:192.158.29.39?transport=udp".to_owned()],
                    username: "unittest".to_owned(),
                    credential: String::new(),
                    credential_type: RTCIceCredentialType::Unspecified,
                },
                Error::ErrNoTurnCredentials,
            ),
        ];

        for (ice_server, expected_err) in tests {
            if let Err(err) = ice_server.urls() {
                assert_eq!(err, expected_err, "{ice_server:?} with err {err:?}");
            } else {
                panic!("expected error, but got ok");
            }
        }
    }

    #[test]
    fn test_ice_server_validate_failure_err_stun_query() {
        let tests = vec![(
            RTCIceServer {
                urls: vec!["stun:google.de?transport=udp".to_owned()],
                username: "unittest".to_owned(),
                credential: String::new(),
                credential_type: RTCIceCredentialType::Oauth,
            },
            ice::Error::ErrStunQuery,
        )];

        for (ice_server, expected_err) in tests {
            if let Err(err) = ice_server.urls() {
                assert_eq!(err, expected_err, "{ice_server:?} with err {err:?}");
            } else {
                panic!("expected error, but got ok");
            }
        }
    }
}
