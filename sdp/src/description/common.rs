use std::fmt;

/// Information describes the "i=" field which provides textual information
/// about the session.
pub type Information = String;

/// ConnectionInformation defines the representation for the "c=" field
/// containing connection data.
#[derive(Debug, Default, Clone)]
pub struct ConnectionInformation {
    pub network_type: String,
    pub address_type: String,
    pub address: Option<Address>,
}

impl fmt::Display for ConnectionInformation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(address) = &self.address {
            write!(f, "{} {} {}", self.network_type, self.address_type, address,)
        } else {
            write!(f, "{} {}", self.network_type, self.address_type,)
        }
    }
}

/// Address describes a structured address token from within the "c=" field.
#[derive(Debug, Default, Clone)]
pub struct Address {
    pub address: String,
    pub ttl: Option<isize>,
    pub range: Option<isize>,
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.address)?;
        if let Some(t) = &self.ttl {
            write!(f, "/{}", t)?;
        }
        if let Some(r) = &self.range {
            write!(f, "/{}", r)?;
        }
        Ok(())
    }
}

/// Bandwidth describes an optional field which denotes the proposed bandwidth
/// to be used by the session or media.
#[derive(Debug, Default, Clone)]
pub struct Bandwidth {
    pub experimental: bool,
    pub bandwidth_type: String,
    pub bandwidth: u64,
}

impl fmt::Display for Bandwidth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let output = if self.experimental { "X-" } else { "" };
        write!(f, "{}{}:{}", output, self.bandwidth_type, self.bandwidth)
    }
}

/// EncryptionKey describes the "k=" which conveys encryption key information.
pub type EncryptionKey = String;

/// Attribute describes the "a=" field which represents the primary means for
/// extending SDP.
#[derive(Debug, Default, Clone)]
pub struct Attribute {
    pub key: String,
    pub value: Option<String>,
}

impl fmt::Display for Attribute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(value) = &self.value {
            write!(f, "{}:{}", self.key, value)
        } else {
            write!(f, "{}", self.key)
        }
    }
}

impl Attribute {
    /// new constructs a new attribute
    pub fn new(key: String, value: Option<String>) -> Self {
        Attribute { key, value }
    }

    /// is_ice_candidate returns true if the attribute key equals "candidate".
    pub fn is_ice_candidate(&self) -> bool {
        self.key.as_str() == "candidate"
    }
}
