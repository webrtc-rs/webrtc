use std::fmt;
use std::io::BufReader;

use crate::ice_candidate::ICECandidate;

use utils::Error;

// Information describes the "i=" field which provides textual information
// about the session.
pub type Information = String;

// ConnectionInformation defines the representation for the "c=" field
// containing connection data.
#[derive(Debug)]
pub struct ConnectionInformation {
    network_type: String,
    address_type: String,
    address: Address,
}

impl fmt::Display for ConnectionInformation {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{} {} {}",
            self.network_type, self.address_type, self.address,
        )
    }
}

// Address desribes a structured address token from within the "c=" field.
#[derive(Debug)]
pub struct Address {
    address: String,
    ttl: Option<i32>,
    range: Option<i32>,
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut parts = vec![self.address.to_owned()];
        if let Some(t) = &self.ttl {
            parts.push(t.to_string());
        }
        if let Some(r) = &self.range {
            parts.push(r.to_string());
        }
        write!(f, "{}", parts.join("/"))
    }
}

// Bandwidth describes an optional field which denotes the proposed bandwidth
// to be used by the session or media.
#[derive(Debug)]
pub struct Bandwidth {
    experimental: bool,
    bandwidth_type: String,
    bandwidth: u64,
}

impl fmt::Display for Bandwidth {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let output = if self.experimental { "X-" } else { "" };
        write!(f, "{}{}:{}", output, self.bandwidth_type, self.bandwidth)
    }
}

// EncryptionKey describes the "k=" which conveys encryption key information.
pub type EncryptionKey = String;

// Attribute describes the "a=" field which represents the primary means for
// extending SDP.
#[derive(Debug)]
pub struct Attribute {
    pub key: String,
    pub value: String,
}

impl fmt::Display for Attribute {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.value.len() > 0 {
            write!(f, "{}:{}", self.key, self.value)
        } else {
            write!(f, "{}", self.key)
        }
    }
}

impl Attribute {
    // constructs a new attribute
    pub fn new(key: String, value: String) -> Self {
        Attribute { key, value }
    }

    // IsICECandidate returns true if the attribute key equals "candidate".
    pub fn is_ice_candidate(&self) -> bool {
        self.key.as_str() == "candidate"
    }

    // ToICECandidate parses the attribute as an ICE Candidate.
    pub fn to_ice_candidate(&self) -> Result<ICECandidate, Error> {
        let mut reader = BufReader::new(self.value.as_bytes());
        let parsed = ICECandidate::unmarshal(&mut reader)?;
        Ok(parsed)
    }
}
