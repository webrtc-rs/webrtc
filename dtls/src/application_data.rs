use std::io::{Read, Write};

use super::content::*;
use crate::error::Result;

// Application data messages are carried by the record layer and are
// fragmented, compressed, and encrypted based on the current connection
// state.  The messages are treated as transparent data to the record
// layer.
// https://tools.ietf.org/html/rfc5246#section-10
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ApplicationData {
    pub data: Vec<u8>,
}

impl ApplicationData {
    pub fn content_type(&self) -> ContentType {
        ContentType::ApplicationData
    }

    pub fn size(&self) -> usize {
        self.data.len()
    }

    pub fn marshal<W: Write>(&self, writer: &mut W) -> Result<()> {
        writer.write_all(&self.data)?;

        Ok(writer.flush()?)
    }

    pub fn unmarshal<R: Read>(reader: &mut R) -> Result<Self> {
        let mut data: Vec<u8> = vec![];
        reader.read_to_end(&mut data)?;

        Ok(ApplicationData { data })
    }
}
