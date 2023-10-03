#[cfg(test)]
mod integrity_test;

use std::fmt;

use md5::{Digest, Md5};
use ring::hmac;

use crate::attributes::*;
use crate::checks::*;
use crate::error::*;
use crate::message::*;

// separator for credentials.
pub(crate) const CREDENTIALS_SEP: &str = ":";

// MessageIntegrity represents MESSAGE-INTEGRITY attribute.
//
// add_to and Check methods are using zero-allocation version of hmac, see
// newHMAC function and internal/hmac/pool.go.
//
// RFC 5389 Section 15.4
#[derive(Default, Clone)]
pub struct MessageIntegrity(pub Vec<u8>);

fn new_hmac(key: &[u8], message: &[u8]) -> Vec<u8> {
    let mac = hmac::Key::new(hmac::HMAC_SHA1_FOR_LEGACY_USE_ONLY, key);
    hmac::sign(&mac, message).as_ref().to_vec()
}

impl fmt::Display for MessageIntegrity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "KEY: 0x{:x?}", self.0)
    }
}

impl Setter for MessageIntegrity {
    // add_to adds MESSAGE-INTEGRITY attribute to message.
    //
    // CPU costly, see BenchmarkMessageIntegrity_AddTo.
    fn add_to(&self, m: &mut Message) -> Result<()> {
        for a in &m.attributes.0 {
            // Message should not contain FINGERPRINT attribute
            // before MESSAGE-INTEGRITY.
            if a.typ == ATTR_FINGERPRINT {
                return Err(Error::ErrFingerprintBeforeIntegrity);
            }
        }
        // The text used as input to HMAC is the STUN message,
        // including the header, up to and including the attribute preceding the
        // MESSAGE-INTEGRITY attribute.
        let length = m.length;
        // Adjusting m.Length to contain MESSAGE-INTEGRITY TLV.
        m.length += (MESSAGE_INTEGRITY_SIZE + ATTRIBUTE_HEADER_SIZE) as u32;
        m.write_length(); // writing length to m.Raw
        let v = new_hmac(&self.0, &m.raw); // calculating HMAC for adjusted m.Raw
        m.length = length; // changing m.Length back

        m.add(ATTR_MESSAGE_INTEGRITY, &v);

        Ok(())
    }
}

pub(crate) const MESSAGE_INTEGRITY_SIZE: usize = 20;

impl MessageIntegrity {
    // new_long_term_integrity returns new MessageIntegrity with key for long-term
    // credentials. Password, username, and realm must be SASL-prepared.
    pub fn new_long_term_integrity(username: String, realm: String, password: String) -> Self {
        let s = [username, realm, password].join(CREDENTIALS_SEP);

        let mut h = Md5::new();
        h.update(s.as_bytes());

        MessageIntegrity(h.finalize().as_slice().to_vec())
    }

    // new_short_term_integrity returns new MessageIntegrity with key for short-term
    // credentials. Password must be SASL-prepared.
    pub fn new_short_term_integrity(password: String) -> Self {
        MessageIntegrity(password.as_bytes().to_vec())
    }

    // Check checks MESSAGE-INTEGRITY attribute.
    //
    // CPU costly, see BenchmarkMessageIntegrity_Check.
    pub fn check(&self, m: &mut Message) -> Result<()> {
        let v = m.get(ATTR_MESSAGE_INTEGRITY)?;

        // Adjusting length in header to match m.Raw that was
        // used when computing HMAC.

        let length = m.length as usize;
        let mut after_integrity = false;
        let mut size_reduced = 0;

        for a in &m.attributes.0 {
            if after_integrity {
                size_reduced += nearest_padded_value_length(a.length as usize);
                size_reduced += ATTRIBUTE_HEADER_SIZE;
            }
            if a.typ == ATTR_MESSAGE_INTEGRITY {
                after_integrity = true;
            }
        }
        m.length -= size_reduced as u32;
        m.write_length();
        // start_of_hmac should be first byte of integrity attribute.
        let start_of_hmac = MESSAGE_HEADER_SIZE + m.length as usize
            - (ATTRIBUTE_HEADER_SIZE + MESSAGE_INTEGRITY_SIZE);
        let b = &m.raw[..start_of_hmac]; // data before integrity attribute
        let expected = new_hmac(&self.0, b);
        m.length = length as u32;
        m.write_length(); // writing length back
        check_hmac(&v, &expected)
    }
}
