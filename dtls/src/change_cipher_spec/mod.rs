#[cfg(test)]
mod change_cipher_spec_test;

use std::io::{Read, Write};

use byteorder::{ReadBytesExt, WriteBytesExt};

use super::content::*;
use super::error::*;

// The change cipher spec protocol exists to signal transitions in
// ciphering strategies.  The protocol consists of a single message,
// which is encrypted and compressed under the current (not the pending)
// connection state.  The message consists of a single byte of value 1.
// https://tools.ietf.org/html/rfc5246#section-7.1
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ChangeCipherSpec;

impl ChangeCipherSpec {
    pub fn content_type(&self) -> ContentType {
        ContentType::ChangeCipherSpec
    }

    pub fn size(&self) -> usize {
        1
    }

    pub fn marshal<W: Write>(&self, writer: &mut W) -> Result<()> {
        writer.write_u8(0x01)?;

        Ok(writer.flush()?)
    }

    pub fn unmarshal<R: Read>(reader: &mut R) -> Result<Self> {
        let data = reader.read_u8()?;
        if data != 0x01 {
            return Err(Error::ErrInvalidCipherSpec);
        }

        Ok(ChangeCipherSpec {})
    }
}
