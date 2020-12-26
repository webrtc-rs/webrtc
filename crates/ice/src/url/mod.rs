#[cfg(test)]
mod url_test;

use crate::errors::*;

use util::Error;

use std::borrow::Cow;
use std::convert::From;
use std::fmt;

// SchemeType indicates the type of server used in the ice.URL structure.
#[derive(PartialEq, Debug)]
pub enum SchemeType {
    // SchemeTypeSTUN indicates the URL represents a STUN server.
    STUN,

    // SchemeTypeSTUNS indicates the URL represents a STUNS (secure) server.
    STUNS,

    // SchemeTypeTURN indicates the URL represents a TURN server.
    TURN,

    // SchemeTypeTURNS indicates the URL represents a TURNS (secure) server.
    TURNS,

    // Unknown defines default public constant to use for "enum" like struct
    // comparisons when no value was defined.
    Unknown,
}

impl From<&str> for SchemeType {
    // NewSchemeType defines a procedure for creating a new SchemeType from a raw
    // string naming the scheme type.
    fn from(raw: &str) -> Self {
        match raw {
            "stun" => SchemeType::STUN,
            "stuns" => SchemeType::STUNS,
            "turn" => SchemeType::TURN,
            "turns" => SchemeType::TURNS,
            _ => SchemeType::Unknown,
        }
    }
}

impl fmt::Display for SchemeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            SchemeType::STUN => "stun",
            SchemeType::STUNS => "stuns",
            SchemeType::TURN => "turn",
            SchemeType::TURNS => "turns",
            _ => "unknown",
        };
        write!(f, "{}", s)
    }
}

// ProtoType indicates the transport protocol type that is used in the ice.URL
// structure.
#[derive(PartialEq, Debug)]
pub enum ProtoType {
    // ProtoTypeUDP indicates the URL uses a UDP transport.
    UDP,

    // ProtoTypeTCP indicates the URL uses a TCP transport.
    TCP,

    Unknown,
}

// defines a procedure for creating a new ProtoType from a raw
// string naming the transport protocol type.
impl From<&str> for ProtoType {
    // NewSchemeType defines a procedure for creating a new SchemeType from a raw
    // string naming the scheme type.
    fn from(raw: &str) -> Self {
        match raw {
            "udp" => ProtoType::UDP,
            "tcp" => ProtoType::TCP,
            _ => ProtoType::Unknown,
        }
    }
}

impl fmt::Display for ProtoType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            ProtoType::UDP => "udp",
            ProtoType::TCP => "tcp",
            _ => "unknown",
        };
        write!(f, "{}", s)
    }
}

// URL represents a STUN (rfc7064) or TURN (rfc7065) URL
#[derive(Debug)]
pub struct URL {
    scheme: SchemeType,
    host: String,
    port: u16,
    username: String,
    password: String,
    proto: ProtoType,
}

impl fmt::Display for URL {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let host = if self.host.contains("::") {
            "[".to_owned() + self.host.as_str() + "]"
        } else {
            self.host.clone()
        };
        if self.scheme == SchemeType::TURN || self.scheme == SchemeType::TURNS {
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

impl URL {
    // ParseURL parses a STUN or TURN urls following the ABNF syntax described in
    // https://tools.ietf.org/html/rfc7064 and https://tools.ietf.org/html/rfc7065
    // respectively.
    pub fn parse_url(raw: &str) -> Result<URL, Error> {
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
        } else if scheme == SchemeType::STUN || scheme == SchemeType::TURN {
            3478
        } else {
            5349
        };

        let mut q_args = raw_parts.query_pairs();
        let proto = match scheme {
            SchemeType::STUN => {
                if q_args.count() > 0 {
                    return Err(ERR_STUN_QUERY.to_owned());
                }
                ProtoType::UDP
            }
            SchemeType::STUNS => {
                if q_args.count() > 0 {
                    return Err(ERR_STUN_QUERY.to_owned());
                }
                ProtoType::TCP
            }
            SchemeType::TURN => {
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
                    ProtoType::UDP
                }
            }
            SchemeType::TURNS => {
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
                    ProtoType::TCP
                }
            }
            _ => {
                return Err(ERR_SCHEME_TYPE.to_owned());
            }
        };

        Ok(URL {
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
        self.scheme == SchemeType::STUNS || self.scheme == SchemeType::TURNS
    }
}
