#[cfg(test)]
mod uri_test;

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
        if let Some(port) = self.port {
            write!(f, "{}:{}:{}", self.scheme, self.host, port)
        } else {
            write!(f, "{}:{}", self.scheme, self.host)
        }
    }
}

impl URI {
    // parse_uri parses URI from string.
    pub fn parse_uri(raw_uri: String) -> Result<Self, Error> {
        // Carefully reusing URI parser from net/url.
        let fields: Vec<&str> = raw_uri.split(':').collect();
        let scheme = if fields.len() <= 1 || fields.len() > 3 {
            return Err(Error::new("invalid stun uri".to_owned()));
        } else if fields[0] != SCHEME && fields[0] != SCHEME_SECURE {
            return Err(Error::new(format!("unknown uri scheme {}", fields[0])));
        } else {
            fields[0].to_owned()
        };
        if fields[1].starts_with("//") {
            return Err(Error::new("unsupported hierarchical".to_owned()));
        }
        let host = fields[1].to_string();
        let port = if fields.len() == 3 {
            Some(fields[2].parse::<u16>()?)
        } else {
            None
        };
        Ok(URI { scheme, host, port })
    }
}
