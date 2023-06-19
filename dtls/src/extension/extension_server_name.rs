#[cfg(test)]
mod extension_server_name_test;

use std::io::{Read, Write};

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use super::*;

const EXTENSION_SERVER_NAME_TYPE_DNSHOST_NAME: u8 = 0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtensionServerName {
    pub(crate) server_name: String,
}

impl ExtensionServerName {
    pub fn extension_value(&self) -> ExtensionValue {
        ExtensionValue::ServerName
    }

    pub fn size(&self) -> usize {
        //TODO: check how to do cryptobyte?
        2 + 2 + 1 + 2 + self.server_name.as_bytes().len()
    }

    pub fn marshal<W: Write>(&self, writer: &mut W) -> Result<()> {
        //TODO: check how to do cryptobyte?
        writer.write_u16::<BigEndian>(2 + 1 + 2 + self.server_name.len() as u16)?;
        writer.write_u16::<BigEndian>(1 + 2 + self.server_name.len() as u16)?;
        writer.write_u8(EXTENSION_SERVER_NAME_TYPE_DNSHOST_NAME)?;
        writer.write_u16::<BigEndian>(self.server_name.len() as u16)?;
        writer.write_all(self.server_name.as_bytes())?;

        Ok(writer.flush()?)
    }

    pub fn unmarshal<R: Read>(reader: &mut R) -> Result<Self> {
        //TODO: check how to do cryptobyte?
        let _ = reader.read_u16::<BigEndian>()? as usize;
        let _ = reader.read_u16::<BigEndian>()? as usize;

        let name_type = reader.read_u8()?;
        if name_type != EXTENSION_SERVER_NAME_TYPE_DNSHOST_NAME {
            return Err(Error::ErrInvalidSniFormat);
        }

        let buf_len = reader.read_u16::<BigEndian>()? as usize;
        let mut buf: Vec<u8> = vec![0u8; buf_len];
        reader.read_exact(&mut buf)?;

        let server_name = String::from_utf8(buf)?;

        Ok(ExtensionServerName { server_name })
    }
}
