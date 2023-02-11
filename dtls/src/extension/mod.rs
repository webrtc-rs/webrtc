pub mod extension_server_name;
pub mod extension_supported_elliptic_curves;
pub mod extension_supported_point_formats;
pub mod extension_supported_signature_algorithms;
pub mod extension_use_extended_master_secret;
pub mod extension_use_srtp;
pub mod renegotiation_info;

use std::io::{Read, Write};

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use extension_server_name::*;
use extension_supported_elliptic_curves::*;
use extension_supported_point_formats::*;
use extension_supported_signature_algorithms::*;
use extension_use_extended_master_secret::*;
use extension_use_srtp::*;

use crate::error::*;
use crate::extension::renegotiation_info::ExtensionRenegotiationInfo;

// https://www.iana.org/assignments/tls-extensiontype-values/tls-extensiontype-values.xhtml
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExtensionValue {
    ServerName = 0,
    SupportedEllipticCurves = 10,
    SupportedPointFormats = 11,
    SupportedSignatureAlgorithms = 13,
    UseSrtp = 14,
    UseExtendedMasterSecret = 23,
    RenegotiationInfo = 65281,
    Unsupported,
}

impl From<u16> for ExtensionValue {
    fn from(val: u16) -> Self {
        match val {
            0 => ExtensionValue::ServerName,
            10 => ExtensionValue::SupportedEllipticCurves,
            11 => ExtensionValue::SupportedPointFormats,
            13 => ExtensionValue::SupportedSignatureAlgorithms,
            14 => ExtensionValue::UseSrtp,
            23 => ExtensionValue::UseExtendedMasterSecret,
            65281 => ExtensionValue::RenegotiationInfo,
            _ => ExtensionValue::Unsupported,
        }
    }
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub enum Extension {
    ServerName(ExtensionServerName),
    SupportedEllipticCurves(ExtensionSupportedEllipticCurves),
    SupportedPointFormats(ExtensionSupportedPointFormats),
    SupportedSignatureAlgorithms(ExtensionSupportedSignatureAlgorithms),
    UseSrtp(ExtensionUseSrtp),
    UseExtendedMasterSecret(ExtensionUseExtendedMasterSecret),
    RenegotiationInfo(ExtensionRenegotiationInfo),
}

impl Extension {
    pub fn extension_value(&self) -> ExtensionValue {
        match self {
            Extension::ServerName(ext) => ext.extension_value(),
            Extension::SupportedEllipticCurves(ext) => ext.extension_value(),
            Extension::SupportedPointFormats(ext) => ext.extension_value(),
            Extension::SupportedSignatureAlgorithms(ext) => ext.extension_value(),
            Extension::UseSrtp(ext) => ext.extension_value(),
            Extension::UseExtendedMasterSecret(ext) => ext.extension_value(),
            Extension::RenegotiationInfo(ext) => ext.extension_value(),
        }
    }

    pub fn size(&self) -> usize {
        let mut len = 2;

        len += match self {
            Extension::ServerName(ext) => ext.size(),
            Extension::SupportedEllipticCurves(ext) => ext.size(),
            Extension::SupportedPointFormats(ext) => ext.size(),
            Extension::SupportedSignatureAlgorithms(ext) => ext.size(),
            Extension::UseSrtp(ext) => ext.size(),
            Extension::UseExtendedMasterSecret(ext) => ext.size(),
            Extension::RenegotiationInfo(ext) => ext.size(),
        };

        len
    }

    pub fn marshal<W: Write>(&self, writer: &mut W) -> Result<()> {
        writer.write_u16::<BigEndian>(self.extension_value() as u16)?;
        match self {
            Extension::ServerName(ext) => ext.marshal(writer),
            Extension::SupportedEllipticCurves(ext) => ext.marshal(writer),
            Extension::SupportedPointFormats(ext) => ext.marshal(writer),
            Extension::SupportedSignatureAlgorithms(ext) => ext.marshal(writer),
            Extension::UseSrtp(ext) => ext.marshal(writer),
            Extension::UseExtendedMasterSecret(ext) => ext.marshal(writer),
            Extension::RenegotiationInfo(ext) => ext.marshal(writer),
        }
    }

    pub fn unmarshal<R: Read>(reader: &mut R) -> Result<Self> {
        let extension_value: ExtensionValue = reader.read_u16::<BigEndian>()?.into();
        match extension_value {
            ExtensionValue::ServerName => Ok(Extension::ServerName(
                ExtensionServerName::unmarshal(reader)?,
            )),
            ExtensionValue::SupportedEllipticCurves => Ok(Extension::SupportedEllipticCurves(
                ExtensionSupportedEllipticCurves::unmarshal(reader)?,
            )),
            ExtensionValue::SupportedPointFormats => Ok(Extension::SupportedPointFormats(
                ExtensionSupportedPointFormats::unmarshal(reader)?,
            )),
            ExtensionValue::SupportedSignatureAlgorithms => {
                Ok(Extension::SupportedSignatureAlgorithms(
                    ExtensionSupportedSignatureAlgorithms::unmarshal(reader)?,
                ))
            }
            ExtensionValue::UseSrtp => Ok(Extension::UseSrtp(ExtensionUseSrtp::unmarshal(reader)?)),
            ExtensionValue::UseExtendedMasterSecret => Ok(Extension::UseExtendedMasterSecret(
                ExtensionUseExtendedMasterSecret::unmarshal(reader)?,
            )),
            ExtensionValue::RenegotiationInfo => Ok(Extension::RenegotiationInfo(
                ExtensionRenegotiationInfo::unmarshal(reader)?,
            )),
            _ => Err(Error::ErrInvalidExtensionType),
        }
    }
}
