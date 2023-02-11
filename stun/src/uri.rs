#[cfg(test)]
mod uri_test;

use std::fmt;

use crate::error::*;

// SCHEME definitions from RFC 7064 Section 3.2.

pub const SCHEME: &str = "stun";
pub const SCHEME_SECURE: &str = "stuns";

// URI as defined in RFC 7064.
#[derive(PartialEq, Eq, Debug)]
pub struct Uri {
    pub scheme: String,
    pub host: String,
    pub port: Option<u16>,
}

impl fmt::Display for Uri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let host = if self.host.contains("::") {
            "[".to_owned() + self.host.as_str() + "]"
        } else {
            self.host.clone()
        };

        if let Some(port) = self.port {
            write!(f, "{}:{}:{}", self.scheme, host, port)
        } else {
            write!(f, "{}:{}", self.scheme, host)
        }
    }
}

impl Uri {
    // parse_uri parses URI from string.
    pub fn parse_uri(raw: &str) -> Result<Self> {
        // work around for url crate
        if raw.contains("//") {
            return Err(Error::ErrInvalidUrl);
        }

        let mut s = raw.to_string();
        let pos = raw.find(':');
        if let Some(p) = pos {
            s.replace_range(p..p + 1, "://");
        } else {
            return Err(Error::ErrSchemeType);
        }

        let raw_parts = url::Url::parse(&s)?;

        let scheme = raw_parts.scheme().into();
        if scheme != SCHEME && scheme != SCHEME_SECURE {
            return Err(Error::ErrSchemeType);
        }

        let host = if let Some(host) = raw_parts.host_str() {
            host.trim()
                .trim_start_matches('[')
                .trim_end_matches(']')
                .to_owned()
        } else {
            return Err(Error::ErrHost);
        };

        let port = raw_parts.port();

        Ok(Uri { scheme, host, port })
    }
}
