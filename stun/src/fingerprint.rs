#[cfg(test)]
mod fingerprint_test;

use crc::{Crc, CRC_32_ISO_HDLC};

use crate::attributes::ATTR_FINGERPRINT;
use crate::checks::*;
use crate::error::*;
use crate::message::*;

// FingerprintAttr represents FINGERPRINT attribute.
//
// RFC 5389 Section 15.5
pub struct FingerprintAttr;

// FINGERPRINT is shorthand for FingerprintAttr.
//
// Example:
//
//  m := New()
//  FINGERPRINT.add_to(m)
pub const FINGERPRINT: FingerprintAttr = FingerprintAttr {};

pub const FINGERPRINT_XOR_VALUE: u32 = 0x5354554e;
pub const FINGERPRINT_SIZE: usize = 4; // 32 bit

// FingerprintValue returns CRC-32 of b XOR-ed by 0x5354554e.
//
// The value of the attribute is computed as the CRC-32 of the STUN message
// up to (but excluding) the FINGERPRINT attribute itself, XOR'ed with
// the 32-bit value 0x5354554e (the XOR helps in cases where an
// application packet is also using CRC-32 in it).
pub fn fingerprint_value(b: &[u8]) -> u32 {
    let checksum = Crc::<u32>::new(&CRC_32_ISO_HDLC).checksum(b);
    checksum ^ FINGERPRINT_XOR_VALUE // XOR
}

impl Setter for FingerprintAttr {
    // add_to adds fingerprint to message.
    fn add_to(&self, m: &mut Message) -> Result<()> {
        let l = m.length;
        // length in header should include size of fingerprint attribute
        m.length += (FINGERPRINT_SIZE + ATTRIBUTE_HEADER_SIZE) as u32; // increasing length
        m.write_length(); // writing Length to Raw
        let val = fingerprint_value(&m.raw);
        let b = val.to_be_bytes();
        m.length = l;
        m.add(ATTR_FINGERPRINT, &b);
        Ok(())
    }
}

impl FingerprintAttr {
    // Check reads fingerprint value from m and checks it, returning error if any.
    // Can return *AttrLengthErr, ErrAttributeNotFound, and *CRCMismatch.
    pub fn check(&self, m: &Message) -> Result<()> {
        let b = m.get(ATTR_FINGERPRINT)?;
        check_size(ATTR_FINGERPRINT, b.len(), FINGERPRINT_SIZE)?;
        let val = u32::from_be_bytes([b[0], b[1], b[2], b[3]]);
        let attr_start = m.raw.len() - (FINGERPRINT_SIZE + ATTRIBUTE_HEADER_SIZE);
        let expected = fingerprint_value(&m.raw[..attr_start]);
        check_fingerprint(val, expected)
    }
}
