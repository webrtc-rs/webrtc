pub(crate) mod generic;
pub(crate) mod h264;

use std::any::Any;
use std::collections::HashMap;
use std::fmt;

use crate::rtp_transceiver::fmtp::generic::GenericFmtp;
use crate::rtp_transceiver::fmtp::h264::H264Fmtp;

/// Fmtp interface for implementing custom
/// Fmtp parsers based on mime_type
pub trait Fmtp: fmt::Debug {
    /// mime_type returns the mime_type associated with
    /// the fmtp
    fn mime_type(&self) -> &str;

    /// match_fmtp compares two fmtp descriptions for
    /// compatibility based on the mime_type    
    fn match_fmtp(&self, f: &(dyn Fmtp)) -> bool;

    /// parameter returns a value for the associated key
    /// if contained in the parsed fmtp string
    fn parameter(&self, key: &str) -> Option<&String>;

    fn equal(&self, other: &(dyn Fmtp)) -> bool;
    fn as_any(&self) -> &(dyn Any);
}

impl PartialEq for dyn Fmtp {
    fn eq(&self, other: &Self) -> bool {
        self.equal(other)
    }
}

/// parse parses an fmtp string based on the MimeType
pub fn parse(mime_type: &str, line: &str) -> Box<dyn Fmtp> {
    let mut parameters = HashMap::new();
    for p in line.split(';').collect::<Vec<&str>>() {
        let pp: Vec<&str> = p.trim().splitn(2, '=').collect();
        let key = pp[0].to_lowercase();
        let value = if pp.len() > 1 {
            pp[1].to_owned()
        } else {
            String::new()
        };
        parameters.insert(key, value);
    }

    if mime_type.to_uppercase() == "video/h264".to_uppercase() {
        Box::new(H264Fmtp { parameters })
    } else {
        Box::new(GenericFmtp {
            mime_type: mime_type.to_owned(),
            parameters,
        })
    }
}
