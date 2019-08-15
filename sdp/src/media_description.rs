use std::{fmt, io};

use utils::Error;

use super::common_description::*;
use super::util::*;

// MediaDescription represents a media type.
// https://tools.ietf.org/html/rfc4566#section-5.14
pub struct MediaDescription {
    // m=<media> <port>/<number of ports> <proto> <fmt> ...
    // https://tools.ietf.org/html/rfc4566#section-5.14
    pub media_name: MediaName,

    // i=<session description>
    // https://tools.ietf.org/html/rfc4566#section-5.4
    pub media_title: Option<Information>,

    // c=<nettype> <addrtype> <connection-address>
    // https://tools.ietf.org/html/rfc4566#section-5.7
    pub connection_information: Option<ConnectionInformation>,

    // b=<bwtype>:<bandwidth>
    // https://tools.ietf.org/html/rfc4566#section-5.8
    pub bandwidth: Vec<Bandwidth>,

    // k=<method>
    // k=<method>:<encryption key>
    // https://tools.ietf.org/html/rfc4566#section-5.12
    pub encryption_key: Option<EncryptionKey>,

    // Attributes are the primary means for extending SDP.  Attributes may
    // be defined to be used as "session-level" attributes, "media-level"
    // attributes, or both.
    // https://tools.ietf.org/html/rfc4566#section-5.12
    pub attributes: Vec<Attribute>,
}

impl MediaDescription {
    // Attribute returns the value of an attribute and if it exists
    pub fn attribute(&self, key: &str) -> Option<&str> {
        for a in &self.attributes {
            if &a.key == key {
                return Some(a.value.as_str());
            }
        }
        return None;
    }

    pub fn marshal(&self) -> String {
        let mut result = String::new();

        result += key_value_build("m=", Some(&self.media_name.to_string())).as_str();
        result += key_value_build("i=", self.media_title.as_ref()).as_str();
        if let Some(connection_information) = &self.connection_information {
            result += key_value_build("c=", Some(&connection_information.to_string())).as_str();
        }
        for bandwidth in &self.bandwidth {
            result += key_value_build("b=", Some(&bandwidth.to_string())).as_str();
        }
        result += key_value_build("k=", self.encryption_key.as_ref()).as_str();
        for attribute in &self.attributes {
            result += key_value_build("a=", Some(&attribute.to_string())).as_str();
        }

        result
    }

    pub fn unmarshal<R: io::BufRead>(reader: &mut R) -> Result<Vec<MediaDescription>, Error> {
        Ok(vec![])
    }
}

// RangedPort supports special format for the media field "m=" port value. If
// it may be necessary to specify multiple transport ports, the protocol allows
// to write it as: <port>/<number of ports> where number of ports is a an
// offsetting range.
pub struct RangedPort {
    value: i32,
    range: Option<i32>,
}

impl fmt::Display for RangedPort {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Some(range) = self.range {
            write!(f, "{}/{}", self.value, range)
        } else {
            write!(f, "{}", self.value)
        }
    }
}

// MediaName describes the "m=" field storage structure.
pub struct MediaName {
    media: String,
    port: RangedPort,
    protos: Vec<String>,
    formats: Vec<String>,
}

impl fmt::Display for MediaName {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let s = vec![
            self.media.clone(),
            self.port.to_string(),
            self.protos.join("/"),
            self.formats.join(" "),
        ];
        write!(f, "{}", s.join(" "))
    }
}
