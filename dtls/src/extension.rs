mod extension_server_name;
mod extension_supported_elliptic_curves;
mod extension_supported_point_formats;
mod extension_supported_signature_algorithms;
mod extension_use_extended_master_secret;
mod extension_use_srtp;

use extension_server_name::*;
use extension_supported_elliptic_curves::*;
use extension_supported_point_formats::*;
use extension_supported_signature_algorithms::*;
use extension_use_extended_master_secret::*;
use extension_use_srtp::*;

use crate::errors::*;

use std::io::{Read, Write};

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use util::Error;

// https://www.iana.org/assignments/tls-extensiontype-values/tls-extensiontype-values.xhtml
#[derive(Clone, Debug, PartialEq)]
pub enum ExtensionValue {
    ServerName = 0,
    SupportedEllipticCurves = 10,
    SupportedPointFormats = 11,
    SupportedSignatureAlgorithms = 13,
    UseSRTP = 14,
    UseExtendedMasterSecret = 23,
    Unsupported,
}

impl From<u16> for ExtensionValue {
    fn from(val: u16) -> Self {
        match val {
            0 => ExtensionValue::ServerName,
            10 => ExtensionValue::SupportedEllipticCurves,
            11 => ExtensionValue::SupportedPointFormats,
            13 => ExtensionValue::SupportedSignatureAlgorithms,
            14 => ExtensionValue::UseSRTP,
            23 => ExtensionValue::UseExtendedMasterSecret,
            _ => ExtensionValue::Unsupported,
        }
    }
}

pub enum Extension {
    ServerName(ExtensionServerName),
    SupportedEllipticCurves(ExtensionSupportedEllipticCurves),
    SupportedPointFormats(ExtensionSupportedPointFormats),
    SupportedSignatureAlgorithms(ExtensionSupportedSignatureAlgorithms),
    UseSRTP(ExtensionUseSRTP),
    UseExtendedMasterSecret(ExtensionUseExtendedMasterSecret),
}

impl Extension {
    pub fn marshal<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        match self {
            Extension::ServerName(ext) => {
                writer.write_u16::<BigEndian>(ExtensionServerName::extension_value() as u16)?;
                ext.marshal(writer)?;
            }

            _ => {}
        }

        Ok(())
    }

    pub fn unmarshal<R: Read>(reader: &mut R) -> Result<Self, Error> {
        let extension_value: ExtensionValue = reader.read_u16::<BigEndian>()?.into();
        match extension_value {
            ExtensionValue::ServerName => Ok(Extension::ServerName(
                ExtensionServerName::unmarshal(reader)?,
            )),
            _ => Err(ERR_INVALID_EXTENSION_TYPE.clone()),
        }
    }
}
