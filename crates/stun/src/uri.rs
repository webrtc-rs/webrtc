#[cfg(test)]
mod uri_test;

use crate::errors::*;

use std::fmt;

use util::Error;

// SCHEME definitions from RFC 7064 Section 3.2.

pub const SCHEME: &str = "stun";
pub const SCHEME_SECURE: &str = "stuns";

// URI as defined in RFC 7064.
#[derive(PartialEq, Debug)]
pub struct URI {
    pub scheme: String,
    pub host: String,
    pub port: Option<u16>,
}

impl fmt::Display for URI {
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

impl URI {
    // parse_uri parses URI from string.
    pub fn parse_uri(raw: &str) -> Result<Self, Error> {
        // work around for url crate
        if raw.contains("//") {
            return Err(ERR_INVALID_URL.to_owned());
        }

        let mut s = raw.to_string();
        let pos = raw.find(':');
        if let Some(p) = pos {
            s.replace_range(p..p + 1, "://");
        } else {
            return Err(ERR_SCHEME_TYPE.to_owned());
        }

        let raw_parts = url::Url::parse(&s)?;

        let scheme = raw_parts.scheme().into();
        if scheme != SCHEME && scheme != SCHEME_SECURE {
            return Err(ERR_SCHEME_TYPE.to_owned());
        }

        let host = if let Some(host) = raw_parts.host_str() {
            host.trim()
                .trim_start_matches('[')
                .trim_end_matches(']')
                .to_owned()
        } else {
            return Err(ERR_HOST.to_owned());
        };

        let port = raw_parts.port();

        Ok(URI { scheme, host, port })
    }
}
