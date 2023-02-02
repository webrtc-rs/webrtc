#[cfg(test)]
mod attributes_test;

use crate::error::*;
use crate::message::*;

use std::fmt;

/// Attributes is list of message attributes.
#[derive(Default, PartialEq, Eq, Debug, Clone)]
pub struct Attributes(pub Vec<RawAttribute>);

impl Attributes {
    /// get returns first attribute from list by the type.
    /// If attribute is present the RawAttribute is returned and the
    /// boolean is true. Otherwise the returned RawAttribute will be
    /// empty and boolean will be false.
    pub fn get(&self, t: AttrType) -> (RawAttribute, bool) {
        for candidate in &self.0 {
            if candidate.typ == t {
                return (candidate.clone(), true);
            }
        }

        (RawAttribute::default(), false)
    }
}

/// AttrType is attribute type.
#[derive(PartialEq, Debug, Eq, Default, Copy, Clone)]
pub struct AttrType(pub u16);

impl fmt::Display for AttrType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let other = format!("0x{:x}", self.0);

        let s = match *self {
            ATTR_MAPPED_ADDRESS => "MAPPED-ADDRESS",
            ATTR_USERNAME => "USERNAME",
            ATTR_ERROR_CODE => "ERROR-CODE",
            ATTR_MESSAGE_INTEGRITY => "MESSAGE-INTEGRITY",
            ATTR_UNKNOWN_ATTRIBUTES => "UNKNOWN-ATTRIBUTES",
            ATTR_REALM => "REALM",
            ATTR_NONCE => "NONCE",
            ATTR_XORMAPPED_ADDRESS => "XOR-MAPPED-ADDRESS",
            ATTR_SOFTWARE => "SOFTWARE",
            ATTR_ALTERNATE_SERVER => "ALTERNATE-SERVER",
            ATTR_FINGERPRINT => "FINGERPRINT",
            ATTR_PRIORITY => "PRIORITY",
            ATTR_USE_CANDIDATE => "USE-CANDIDATE",
            ATTR_ICE_CONTROLLED => "ICE-CONTROLLED",
            ATTR_ICE_CONTROLLING => "ICE-CONTROLLING",
            ATTR_CHANNEL_NUMBER => "CHANNEL-NUMBER",
            ATTR_LIFETIME => "LIFETIME",
            ATTR_XOR_PEER_ADDRESS => "XOR-PEER-ADDRESS",
            ATTR_DATA => "DATA",
            ATTR_XOR_RELAYED_ADDRESS => "XOR-RELAYED-ADDRESS",
            ATTR_EVEN_PORT => "EVEN-PORT",
            ATTR_REQUESTED_TRANSPORT => "REQUESTED-TRANSPORT",
            ATTR_DONT_FRAGMENT => "DONT-FRAGMENT",
            ATTR_RESERVATION_TOKEN => "RESERVATION-TOKEN",
            ATTR_CONNECTION_ID => "CONNECTION-ID",
            ATTR_REQUESTED_ADDRESS_FAMILY => "REQUESTED-ADDRESS-FAMILY",
            ATTR_MESSAGE_INTEGRITY_SHA256 => "MESSAGE-INTEGRITY-SHA256",
            ATTR_PASSWORD_ALGORITHM => "PASSWORD-ALGORITHM",
            ATTR_USER_HASH => "USERHASH",
            ATTR_PASSWORD_ALGORITHMS => "PASSWORD-ALGORITHMS",
            ATTR_ALTERNATE_DOMAIN => "ALTERNATE-DOMAIN",
            _ => other.as_str(),
        };

        write!(f, "{s}")
    }
}

impl AttrType {
    /// required returns true if type is from comprehension-required range (0x0000-0x7FFF).
    pub fn required(&self) -> bool {
        self.0 <= 0x7FFF
    }

    /// optional returns true if type is from comprehension-optional range (0x8000-0xFFFF).
    pub fn optional(&self) -> bool {
        self.0 >= 0x8000
    }

    /// value returns uint16 representation of attribute type.
    pub fn value(&self) -> u16 {
        self.0
    }
}

/// Attributes from comprehension-required range (0x0000-0x7FFF).
pub const ATTR_MAPPED_ADDRESS: AttrType = AttrType(0x0001); // MAPPED-ADDRESS
pub const ATTR_USERNAME: AttrType = AttrType(0x0006); // USERNAME
pub const ATTR_MESSAGE_INTEGRITY: AttrType = AttrType(0x0008); // MESSAGE-INTEGRITY
pub const ATTR_ERROR_CODE: AttrType = AttrType(0x0009); // ERROR-CODE
pub const ATTR_UNKNOWN_ATTRIBUTES: AttrType = AttrType(0x000A); // UNKNOWN-ATTRIBUTES
pub const ATTR_REALM: AttrType = AttrType(0x0014); // REALM
pub const ATTR_NONCE: AttrType = AttrType(0x0015); // NONCE
pub const ATTR_XORMAPPED_ADDRESS: AttrType = AttrType(0x0020); // XOR-MAPPED-ADDRESS

/// Attributes from comprehension-optional range (0x8000-0xFFFF).
pub const ATTR_SOFTWARE: AttrType = AttrType(0x8022); // SOFTWARE
pub const ATTR_ALTERNATE_SERVER: AttrType = AttrType(0x8023); // ALTERNATE-SERVER
pub const ATTR_FINGERPRINT: AttrType = AttrType(0x8028); // FINGERPRINT

/// Attributes from RFC 5245 ICE.
pub const ATTR_PRIORITY: AttrType = AttrType(0x0024); // PRIORITY
pub const ATTR_USE_CANDIDATE: AttrType = AttrType(0x0025); // USE-CANDIDATE
pub const ATTR_ICE_CONTROLLED: AttrType = AttrType(0x8029); // ICE-CONTROLLED
pub const ATTR_ICE_CONTROLLING: AttrType = AttrType(0x802A); // ICE-CONTROLLING

/// Attributes from RFC 5766 TURN.
pub const ATTR_CHANNEL_NUMBER: AttrType = AttrType(0x000C); // CHANNEL-NUMBER
pub const ATTR_LIFETIME: AttrType = AttrType(0x000D); // LIFETIME
pub const ATTR_XOR_PEER_ADDRESS: AttrType = AttrType(0x0012); // XOR-PEER-ADDRESS
pub const ATTR_DATA: AttrType = AttrType(0x0013); // DATA
pub const ATTR_XOR_RELAYED_ADDRESS: AttrType = AttrType(0x0016); // XOR-RELAYED-ADDRESS
pub const ATTR_EVEN_PORT: AttrType = AttrType(0x0018); // EVEN-PORT
pub const ATTR_REQUESTED_TRANSPORT: AttrType = AttrType(0x0019); // REQUESTED-TRANSPORT
pub const ATTR_DONT_FRAGMENT: AttrType = AttrType(0x001A); // DONT-FRAGMENT
pub const ATTR_RESERVATION_TOKEN: AttrType = AttrType(0x0022); // RESERVATION-TOKEN

/// Attributes from RFC 5780 NAT Behavior Discovery
pub const ATTR_CHANGE_REQUEST: AttrType = AttrType(0x0003); // CHANGE-REQUEST
pub const ATTR_PADDING: AttrType = AttrType(0x0026); // PADDING
pub const ATTR_RESPONSE_PORT: AttrType = AttrType(0x0027); // RESPONSE-PORT
pub const ATTR_CACHE_TIMEOUT: AttrType = AttrType(0x8027); // CACHE-TIMEOUT
pub const ATTR_RESPONSE_ORIGIN: AttrType = AttrType(0x802b); // RESPONSE-ORIGIN
pub const ATTR_OTHER_ADDRESS: AttrType = AttrType(0x802C); // OTHER-ADDRESS

/// Attributes from RFC 3489, removed by RFC 5389,
///  but still used by RFC5389-implementing software like Vovida.org, reTURNServer, etc.
pub const ATTR_SOURCE_ADDRESS: AttrType = AttrType(0x0004); // SOURCE-ADDRESS
pub const ATTR_CHANGED_ADDRESS: AttrType = AttrType(0x0005); // CHANGED-ADDRESS

/// Attributes from RFC 6062 TURN Extensions for TCP Allocations.
pub const ATTR_CONNECTION_ID: AttrType = AttrType(0x002a); // CONNECTION-ID

/// Attributes from RFC 6156 TURN IPv6.
pub const ATTR_REQUESTED_ADDRESS_FAMILY: AttrType = AttrType(0x0017); // REQUESTED-ADDRESS-FAMILY

/// Attributes from An Origin Attribute for the STUN Protocol.
pub const ATTR_ORIGIN: AttrType = AttrType(0x802F);

/// Attributes from RFC 8489 STUN.
pub const ATTR_MESSAGE_INTEGRITY_SHA256: AttrType = AttrType(0x001C); // MESSAGE-INTEGRITY-SHA256
pub const ATTR_PASSWORD_ALGORITHM: AttrType = AttrType(0x001D); // PASSWORD-ALGORITHM
pub const ATTR_USER_HASH: AttrType = AttrType(0x001E); // USER-HASH
pub const ATTR_PASSWORD_ALGORITHMS: AttrType = AttrType(0x8002); // PASSWORD-ALGORITHMS
pub const ATTR_ALTERNATE_DOMAIN: AttrType = AttrType(0x8003); // ALTERNATE-DOMAIN

/// RawAttribute is a Type-Length-Value (TLV) object that
/// can be added to a STUN message. Attributes are divided into two
/// types: comprehension-required and comprehension-optional.  STUN
/// agents can safely ignore comprehension-optional attributes they
/// don't understand, but cannot successfully process a message if it
/// contains comprehension-required attributes that are not
/// understood.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct RawAttribute {
    pub typ: AttrType,
    pub length: u16, // ignored while encoding
    pub value: Vec<u8>,
}

impl fmt::Display for RawAttribute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {:?}", self.typ, self.value)
    }
}

impl Setter for RawAttribute {
    /// add_to implements Setter, adding attribute as a.Type with a.Value and ignoring
    /// the Length field.
    fn add_to(&self, m: &mut Message) -> Result<()> {
        m.add(self.typ, &self.value);
        Ok(())
    }
}

pub(crate) const PADDING: usize = 4;

/// STUN aligns attributes on 32-bit boundaries, attributes whose content
/// is not a multiple of 4 bytes are padded with 1, 2, or 3 bytes of
/// padding so that its value contains a multiple of 4 bytes.  The
/// padding bits are ignored, and may be any value.
///
/// https://tools.ietf.org/html/rfc5389#section-15
pub(crate) fn nearest_padded_value_length(l: usize) -> usize {
    let mut n = PADDING * (l / PADDING);
    if n < l {
        n += PADDING
    }
    n
}

/// This method converts uint16 vlue to AttrType. If it finds an old attribute
/// type value, it also translates it to the new value to enable backward
/// compatibility. (See: https://github.com/pion/stun/issues/21)
pub(crate) fn compat_attr_type(val: u16) -> AttrType {
    if val == 0x8020 {
        // draft-ietf-behave-rfc3489bis-02, MS-TURN
        ATTR_XORMAPPED_ADDRESS // new: 0x0020 (from draft-ietf-behave-rfc3489bis-03 on)
    } else {
        AttrType(val)
    }
}
