use std::io::{Read, Write};

use util::Error;

// https://tools.ietf.org/html/rfc4346#section-6.2.1
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum ContentType {
    ChangeCipherSpec = 20,
    Alert = 21,
    Handshake = 22,
    ApplicationData = 23,
}

pub trait Content {
    fn content_type() -> ContentType;
    fn marshal<W: Write>(&self, writer: &mut W) -> Result<(), Error>;
    fn unmarshal<R: Read>(reader: &mut R) -> Result<Self, Error>
    where
        Self: std::marker::Sized;
}
