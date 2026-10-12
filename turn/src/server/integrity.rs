//! RFC 8489 message integrity for the legacy and SHA-256 wire attributes.

use ring::hmac;
use stun::attributes::{ATTR_MESSAGE_INTEGRITY, ATTR_MESSAGE_INTEGRITY_SHA256};
use stun::integrity::MessageIntegrity;
use stun::message::{Message, Setter};
use subtle::ConstantTimeEq;

#[derive(Clone)]
pub(crate) struct Integrity {
    pub key: Vec<u8>,
    pub password_algorithm: u16,
    pub sha256: bool,
}

impl Integrity {
    pub fn check(&self, message: &Message) -> Result<(), stun::Error> {
        if !self.sha256 {
            if message.get(ATTR_MESSAGE_INTEGRITY)?.len() != 20 {
                return Err(stun::Error::Other(
                    "invalid MESSAGE-INTEGRITY length".into(),
                ));
            }
            return MessageIntegrity(self.key.clone()).check(&mut message.clone());
        }
        let value = message.get(ATTR_MESSAGE_INTEGRITY_SHA256)?;
        // RFC 8489 section 14.6 permits truncation only when the STUN usage
        // explicitly allows it. TURN does not define a truncated digest.
        if value.len() != 32 {
            return Err(stun::Error::Other(
                "invalid MESSAGE-INTEGRITY-SHA256 length".into(),
            ));
        }
        let mut offset = 20;
        while offset + 4 <= message.raw.len() {
            let typ = u16::from_be_bytes([message.raw[offset], message.raw[offset + 1]]);
            if typ == ATTR_MESSAGE_INTEGRITY_SHA256.0 {
                break;
            }
            let length =
                u16::from_be_bytes([message.raw[offset + 2], message.raw[offset + 3]]) as usize;
            offset += 4 + length.div_ceil(4) * 4;
        }
        let mut data = message.raw[..offset].to_vec();
        data[2..4].copy_from_slice(&((offset - 20 + 4 + value.len()) as u16).to_be_bytes());
        let key = hmac::Key::new(hmac::HMAC_SHA256, &self.key);
        let expected = hmac::sign(&key, &data);
        if expected.as_ref()[..value.len()].ct_eq(&value).into() {
            Ok(())
        } else {
            Err(stun::Error::Other("message integrity mismatch".into()))
        }
    }
}

impl Setter for Integrity {
    fn add_to(&self, message: &mut Message) -> Result<(), stun::Error> {
        if !self.sha256 {
            return MessageIntegrity(self.key.clone()).add_to(message);
        }
        let mut data = message.raw.clone();
        data[2..4].copy_from_slice(&((message.length + 36) as u16).to_be_bytes());
        let key = hmac::Key::new(hmac::HMAC_SHA256, &self.key);
        let value = hmac::sign(&key, &data);
        message.add(ATTR_MESSAGE_INTEGRITY_SHA256, value.as_ref());
        Ok(())
    }
}
