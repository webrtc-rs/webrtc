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
