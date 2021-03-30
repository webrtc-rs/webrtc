#[cfg(test)]
mod url_test;

use crate::errors::*;

use util::Error;

use std::borrow::Cow;
use std::convert::From;
use std::fmt;

// SchemeType indicates the type of server used in the ice.URL structure.
#[derive(PartialEq, Debug, Copy, Clone)]
pub enum SchemeType {
    // SchemeTypeSTUN indicates the URL represents a STUN server.
    Stun,

    // SchemeTypeSTUNS indicates the URL represents a STUNS (secure) server.
    Stuns,

    // SchemeTypeTURN indicates the URL represents a TURN server.
    Turn,

    // SchemeTypeTURNS indicates the URL represents a TURNS (secure) server.
    Turns,

    // Unknown defines default public constant to use for "enum" like struct
    // comparisons when no value was defined.
    Unknown,
}

impl Default for SchemeType {
    fn default() -> Self {
        SchemeType::Unknown
    }
}

impl From<&str> for SchemeType {
    // NewSchemeType defines a procedure for creating a new SchemeType from a raw
    // string naming the scheme type.
    fn from(raw: &str) -> Self {
        match raw {
            "stun" => SchemeType::Stun,
            "stuns" => SchemeType::Stuns,
            "turn" => SchemeType::Turn,
            "turns" => SchemeType::Turns,
            _ => SchemeType::Unknown,
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
            _ => "unknown",
        };
        write!(f, "{}", s)
    }
}

// ProtoType indicates the transport protocol type that is used in the ice.URL
// structure.
#[derive(PartialEq, Debug, Copy, Clone)]
pub enum ProtoType {
    // ProtoTypeUDP indicates the URL uses a UDP transport.
    Udp,

    // ProtoTypeTCP indicates the URL uses a TCP transport.
    Tcp,

    Unknown,
}

impl Default for ProtoType {
    fn default() -> Self {
        ProtoType::Udp
    }
}

// defines a procedure for creating a new ProtoType from a raw
// string naming the transport protocol type.
impl From<&str> for ProtoType {
    // NewSchemeType defines a procedure for creating a new SchemeType from a raw
    // string naming the scheme type.
    fn from(raw: &str) -> Self {
        match raw {
            "udp" => ProtoType::Udp,
            "tcp" => ProtoType::Tcp,
            _ => ProtoType::Unknown,
        }
    }
}

impl fmt::Display for ProtoType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            ProtoType::Udp => "udp",
            ProtoType::Tcp => "tcp",
            _ => "unknown",
        };
        write!(f, "{}", s)
    }
}

// URL represents a STUN (rfc7064) or TURN (rfc7065) URL
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
    // ParseURL parses a STUN or TURN urls following the ABNF syntax described in
    // https://tools.ietf.org/html/rfc7064 and https://tools.ietf.org/html/rfc7065
    // respectively.
    pub fn parse_url(raw: &str) -> Result<Url, Error> {
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

        let host = if let Some(host) = raw_parts.host_str() {
            host.trim()
                .trim_start_matches('[')
                .trim_end_matches(']')
                .to_owned()
        } else {
            return Err(ERR_HOST.to_owned());
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
                    return Err(ERR_STUN_QUERY.to_owned());
                }
                ProtoType::Udp
            }
            SchemeType::Stuns => {
                if q_args.count() > 0 {
                    return Err(ERR_STUN_QUERY.to_owned());
                }
                ProtoType::Tcp
            }
            SchemeType::Turn => {
                if q_args.count() > 1 {
                    return Err(ERR_INVALID_QUERY.to_owned());
                }
                if let Some((key, value)) = q_args.next() {
                    if key == Cow::Borrowed("transport") {
                        let proto: ProtoType = value.as_ref().into();
                        if proto == ProtoType::Unknown {
                            return Err(ERR_PROTO_TYPE.to_owned());
                        } else {
                            proto
                        }
                    } else {
                        return Err(ERR_INVALID_QUERY.to_owned());
                    }
                } else {
                    ProtoType::Udp
                }
            }
            SchemeType::Turns => {
                if q_args.count() > 1 {
                    return Err(ERR_INVALID_QUERY.to_owned());
                }
                if let Some((key, value)) = q_args.next() {
                    if key == Cow::Borrowed("transport") {
                        let proto: ProtoType = value.as_ref().into();
                        if proto == ProtoType::Unknown {
                            return Err(ERR_PROTO_TYPE.to_owned());
                        } else {
                            proto
                        }
                    } else {
                        return Err(ERR_INVALID_QUERY.to_owned());
                    }
                } else {
                    ProtoType::Tcp
                }
            }
            _ => {
                return Err(ERR_SCHEME_TYPE.to_owned());
            }
        };

        Ok(Url {
            scheme,
            host,
            port,
            username: "".to_owned(),
            password: "".to_owned(),
            proto,
        })
    }

    /*
    fn parse_proto(raw:&str) ->Result<ProtoType, Error> {
        let qArgs= raw.split('=');
        if qArgs.len() != 2 {
            return Err(ERR_INVALID_QUERY.to_owned());
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

    // is_secure returns whether the this URL's scheme describes secure scheme or not.
    pub fn is_secure(&self) -> bool {
        self.scheme == SchemeType::Stuns || self.scheme == SchemeType::Turns
    }
}
