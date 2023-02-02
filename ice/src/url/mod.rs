#[cfg(test)]
mod url_test;

use crate::error::*;

use std::borrow::Cow;
use std::convert::From;
use std::fmt;

/// The type of server used in the ice.URL structure.
#[derive(PartialEq, Eq, Debug, Copy, Clone)]
pub enum SchemeType {
    /// The URL represents a STUN server.
    Stun,

    /// The URL represents a STUNS (secure) server.
    Stuns,

    /// The URL represents a TURN server.
    Turn,

    /// The URL represents a TURNS (secure) server.
    Turns,

    /// Default public constant to use for "enum" like struct comparisons when no value was defined.
    Unknown,
}

impl Default for SchemeType {
    fn default() -> Self {
        Self::Unknown
    }
}

impl From<&str> for SchemeType {
    /// Defines a procedure for creating a new `SchemeType` from a raw
    /// string naming the scheme type.
    fn from(raw: &str) -> Self {
        match raw {
            "stun" => Self::Stun,
            "stuns" => Self::Stuns,
            "turn" => Self::Turn,
            "turns" => Self::Turns,
            _ => Self::Unknown,
        }
    }
}

impl fmt::Display for SchemeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            SchemeType::Stun => "stun",
            SchemeType::Stuns => "stuns",
            SchemeType::Turn => "turn",
            SchemeType::Turns => "turns",
            SchemeType::Unknown => "unknown",
        };
        write!(f, "{s}")
    }
}

/// The transport protocol type that is used in the `ice::url::Url` structure.
#[derive(PartialEq, Eq, Debug, Copy, Clone)]
pub enum ProtoType {
    /// The URL uses a UDP transport.
    Udp,

    /// The URL uses a TCP transport.
    Tcp,

    Unknown,
}

impl Default for ProtoType {
    fn default() -> Self {
        Self::Udp
    }
}

// defines a procedure for creating a new ProtoType from a raw
// string naming the transport protocol type.
impl From<&str> for ProtoType {
    // NewSchemeType defines a procedure for creating a new SchemeType from a raw
    // string naming the scheme type.
    fn from(raw: &str) -> Self {
        match raw {
            "udp" => Self::Udp,
            "tcp" => Self::Tcp,
            _ => Self::Unknown,
        }
    }
}

impl fmt::Display for ProtoType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            Self::Udp => "udp",
            Self::Tcp => "tcp",
            Self::Unknown => "unknown",
        };
        write!(f, "{s}")
    }
}

/// Represents a STUN (rfc7064) or TURN (rfc7065) URL.
#[derive(Debug, Clone, Default)]
pub struct Url {
    pub scheme: SchemeType,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub proto: ProtoType,
}

impl fmt::Display for Url {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let host = if self.host.contains("::") {
            "[".to_owned() + self.host.as_str() + "]"
        } else {
            self.host.clone()
        };
        if self.scheme == SchemeType::Turn || self.scheme == SchemeType::Turns {
            write!(
                f,
                "{}:{}:{}?transport={}",
                self.scheme, host, self.port, self.proto
            )
        } else {
            write!(f, "{}:{}:{}", self.scheme, host, self.port)
        }
    }
}

impl Url {
    /// Parses a STUN or TURN urls following the ABNF syntax described in
    /// [IETF rfc-7064](https://tools.ietf.org/html/rfc7064) and
    /// [IETF rfc-7065](https://tools.ietf.org/html/rfc7065) respectively.
    pub fn parse_url(raw: &str) -> Result<Self> {
        // work around for url crate
        if raw.contains("//") {
            return Err(Error::ErrInvalidUrl);
        }

        let mut s = raw.to_string();
        let pos = raw.find(':');
        if let Some(p) = pos {
            s.replace_range(p..=p, "://");
        } else {
            return Err(Error::ErrSchemeType);
        }

        let raw_parts = url::Url::parse(&s)?;

        let scheme = raw_parts.scheme().into();

        let host = if let Some(host) = raw_parts.host_str() {
            host.trim()
                .trim_start_matches('[')
                .trim_end_matches(']')
                .to_owned()
        } else {
            return Err(Error::ErrHost);
        };

        let port = if let Some(port) = raw_parts.port() {
            port
        } else if scheme == SchemeType::Stun || scheme == SchemeType::Turn {
            3478
        } else {
            5349
        };

        let mut q_args = raw_parts.query_pairs();
        let proto = match scheme {
            SchemeType::Stun => {
                if q_args.count() > 0 {
                    return Err(Error::ErrStunQuery);
                }
                ProtoType::Udp
            }
            SchemeType::Stuns => {
                if q_args.count() > 0 {
                    return Err(Error::ErrStunQuery);
                }
                ProtoType::Tcp
            }
            SchemeType::Turn => {
                if q_args.count() > 1 {
                    return Err(Error::ErrInvalidQuery);
                }
                if let Some((key, value)) = q_args.next() {
                    if key == Cow::Borrowed("transport") {
                        let proto: ProtoType = value.as_ref().into();
                        if proto == ProtoType::Unknown {
                            return Err(Error::ErrProtoType);
                        }
                        proto
                    } else {
                        return Err(Error::ErrInvalidQuery);
                    }
                } else {
                    ProtoType::Udp
                }
            }
            SchemeType::Turns => {
                if q_args.count() > 1 {
                    return Err(Error::ErrInvalidQuery);
                }
                if let Some((key, value)) = q_args.next() {
                    if key == Cow::Borrowed("transport") {
                        let proto: ProtoType = value.as_ref().into();
                        if proto == ProtoType::Unknown {
                            return Err(Error::ErrProtoType);
                        }
                        proto
                    } else {
                        return Err(Error::ErrInvalidQuery);
                    }
                } else {
                    ProtoType::Tcp
                }
            }
            SchemeType::Unknown => {
                return Err(Error::ErrSchemeType);
            }
        };

        Ok(Self {
            scheme,
            host,
            port,
            username: "".to_owned(),
            password: "".to_owned(),
            proto,
        })
    }

    /*
    fn parse_proto(raw:&str) ->Result<ProtoType> {
        let qArgs= raw.split('=');
        if qArgs.len() != 2 {
            return Err(Error::ErrInvalidQuery.into());
        }

        var proto ProtoType
        if rawProto := qArgs.Get("transport"); rawProto != "" {
            if proto = NewProtoType(rawProto); proto == ProtoType(0) {
                return ProtoType(Unknown), ErrProtoType
            }
            return proto, nil
        }

        if len(qArgs) > 0 {
            return ProtoType(Unknown), ErrInvalidQuery
        }

        return proto, nil
    }*/

    /// Returns whether the this URL's scheme describes secure scheme or not.
    #[must_use]
    pub fn is_secure(&self) -> bool {
        self.scheme == SchemeType::Stuns || self.scheme == SchemeType::Turns
    }
}
