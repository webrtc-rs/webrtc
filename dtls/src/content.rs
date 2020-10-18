use super::alert::*;
use super::application_data::*;
use super::change_cipher_spec::*;
use super::handshake::*;

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

/*
pub trait Content {
    fn content_type() -> ContentType;
    fn marshal<W: Write>(&self, writer: &mut W) -> Result<(), Error>;
    fn unmarshal<R: Read>(reader: &mut R) -> Result<Self, Error>
    where
        Self: std::marker::Sized;
}*/

#[derive(Clone, PartialEq, Debug)]
pub enum Content {
    ChangeCipherSpec(ChangeCipherSpec),
    Alert(Alert),
    Handshake(Handshake),
    ApplicationData(ApplicationData),
}

impl Content {
    pub fn content_type(&self) -> ContentType {
        match self {
            Content::ChangeCipherSpec(c) => c.content_type(),
            Content::Alert(c) => c.content_type(),
            Content::Handshake(c) => c.content_type(),
            Content::ApplicationData(c) => c.content_type(),
        }
    }

    pub fn marshal<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        match self {
            Content::ChangeCipherSpec(c) => c.marshal(writer),
            Content::Alert(c) => c.marshal(writer),
            Content::Handshake(c) => c.marshal(writer),
            Content::ApplicationData(c) => c.marshal(writer),
        }
    }

    pub fn unmarshal<R: Read>(content_type: ContentType, reader: &mut R) -> Result<Self, Error> {
        match content_type {
            ContentType::ChangeCipherSpec => Ok(Content::ChangeCipherSpec(
                ChangeCipherSpec::unmarshal(reader)?,
            )),
            ContentType::Alert => Ok(Content::Alert(Alert::unmarshal(reader)?)),
            ContentType::Handshake => Ok(Content::Handshake(Handshake::unmarshal(reader)?)),
            ContentType::ApplicationData => Ok(Content::ApplicationData(
                ApplicationData::unmarshal(reader)?,
            )),
        }
    }
}
