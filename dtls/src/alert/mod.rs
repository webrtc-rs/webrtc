#[cfg(test)]
mod alert_test;

use std::fmt;
use std::io::{Read, Write};

use byteorder::{ReadBytesExt, WriteBytesExt};

use super::content::*;
use crate::error::Result;

#[derive(Copy, Clone, PartialEq, Debug)]
pub(crate) enum AlertLevel {
    Warning = 1,
    Fatal = 2,
    Invalid,
}

impl fmt::Display for AlertLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            AlertLevel::Warning => write!(f, "LevelWarning"),
            AlertLevel::Fatal => write!(f, "LevelFatal"),
            _ => write!(f, "Invalid alert level"),
        }
    }
}

impl From<u8> for AlertLevel {
    fn from(val: u8) -> Self {
        match val {
            1 => AlertLevel::Warning,
            2 => AlertLevel::Fatal,
            _ => AlertLevel::Invalid,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub(crate) enum AlertDescription {
    CloseNotify = 0,
    UnexpectedMessage = 10,
    BadRecordMac = 20,
    DecryptionFailed = 21,
    RecordOverflow = 22,
    DecompressionFailure = 30,
    HandshakeFailure = 40,
    NoCertificate = 41,
    BadCertificate = 42,
    UnsupportedCertificate = 43,
    CertificateRevoked = 44,
    CertificateExpired = 45,
    CertificateUnknown = 46,
    IllegalParameter = 47,
    UnknownCa = 48,
    AccessDenied = 49,
    DecodeError = 50,
    DecryptError = 51,
    ExportRestriction = 60,
    ProtocolVersion = 70,
    InsufficientSecurity = 71,
    InternalError = 80,
    UserCanceled = 90,
    NoRenegotiation = 100,
    UnsupportedExtension = 110,
    UnknownPskIdentity = 115,
    Invalid,
}

impl fmt::Display for AlertDescription {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            AlertDescription::CloseNotify => write!(f, "CloseNotify"),
            AlertDescription::UnexpectedMessage => write!(f, "UnexpectedMessage"),
            AlertDescription::BadRecordMac => write!(f, "BadRecordMac"),
            AlertDescription::DecryptionFailed => write!(f, "DecryptionFailed"),
            AlertDescription::RecordOverflow => write!(f, "RecordOverflow"),
            AlertDescription::DecompressionFailure => write!(f, "DecompressionFailure"),
            AlertDescription::HandshakeFailure => write!(f, "HandshakeFailure"),
            AlertDescription::NoCertificate => write!(f, "NoCertificate"),
            AlertDescription::BadCertificate => write!(f, "BadCertificate"),
            AlertDescription::UnsupportedCertificate => write!(f, "UnsupportedCertificate"),
            AlertDescription::CertificateRevoked => write!(f, "CertificateRevoked"),
            AlertDescription::CertificateExpired => write!(f, "CertificateExpired"),
            AlertDescription::CertificateUnknown => write!(f, "CertificateUnknown"),
            AlertDescription::IllegalParameter => write!(f, "IllegalParameter"),
            AlertDescription::UnknownCa => write!(f, "UnknownCA"),
            AlertDescription::AccessDenied => write!(f, "AccessDenied"),
            AlertDescription::DecodeError => write!(f, "DecodeError"),
            AlertDescription::DecryptError => write!(f, "DecryptError"),
            AlertDescription::ExportRestriction => write!(f, "ExportRestriction"),
            AlertDescription::ProtocolVersion => write!(f, "ProtocolVersion"),
            AlertDescription::InsufficientSecurity => write!(f, "InsufficientSecurity"),
            AlertDescription::InternalError => write!(f, "InternalError"),
            AlertDescription::UserCanceled => write!(f, "UserCanceled"),
            AlertDescription::NoRenegotiation => write!(f, "NoRenegotiation"),
            AlertDescription::UnsupportedExtension => write!(f, "UnsupportedExtension"),
            AlertDescription::UnknownPskIdentity => write!(f, "UnknownPskIdentity"),
            _ => write!(f, "Invalid alert description"),
        }
    }
}

impl From<u8> for AlertDescription {
    fn from(val: u8) -> Self {
        match val {
            0 => AlertDescription::CloseNotify,
            10 => AlertDescription::UnexpectedMessage,
            20 => AlertDescription::BadRecordMac,
            21 => AlertDescription::DecryptionFailed,
            22 => AlertDescription::RecordOverflow,
            30 => AlertDescription::DecompressionFailure,
            40 => AlertDescription::HandshakeFailure,
            41 => AlertDescription::NoCertificate,
            42 => AlertDescription::BadCertificate,
            43 => AlertDescription::UnsupportedCertificate,
            44 => AlertDescription::CertificateRevoked,
            45 => AlertDescription::CertificateExpired,
            46 => AlertDescription::CertificateUnknown,
            47 => AlertDescription::IllegalParameter,
            48 => AlertDescription::UnknownCa,
            49 => AlertDescription::AccessDenied,
            50 => AlertDescription::DecodeError,
            51 => AlertDescription::DecryptError,
            60 => AlertDescription::ExportRestriction,
            70 => AlertDescription::ProtocolVersion,
            71 => AlertDescription::InsufficientSecurity,
            80 => AlertDescription::InternalError,
            90 => AlertDescription::UserCanceled,
            100 => AlertDescription::NoRenegotiation,
            110 => AlertDescription::UnsupportedExtension,
            115 => AlertDescription::UnknownPskIdentity,
            _ => AlertDescription::Invalid,
        }
    }
}

// One of the content types supported by the TLS record layer is the
// alert type.  Alert messages convey the severity of the message
// (warning or fatal) and a description of the alert.  Alert messages
// with a level of fatal result in the immediate termination of the
// connection.  In this case, other connections corresponding to the
// session may continue, but the session identifier MUST be invalidated,
// preventing the failed session from being used to establish new
// connections.  Like other messages, alert messages are encrypted and
// compressed, as specified by the current connection state.
// https://tools.ietf.org/html/rfc5246#section-7.2
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct Alert {
    pub(crate) alert_level: AlertLevel,
    pub(crate) alert_description: AlertDescription,
}

impl fmt::Display for Alert {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Alert {}: {}", self.alert_level, self.alert_description)
    }
}

impl Alert {
    pub fn content_type(&self) -> ContentType {
        ContentType::Alert
    }

    pub fn size(&self) -> usize {
        2
    }

    pub fn marshal<W: Write>(&self, writer: &mut W) -> Result<()> {
        writer.write_u8(self.alert_level as u8)?;
        writer.write_u8(self.alert_description as u8)?;

        Ok(writer.flush()?)
    }

    pub fn unmarshal<R: Read>(reader: &mut R) -> Result<Self> {
        let alert_level = reader.read_u8()?.into();
        let alert_description = reader.read_u8()?.into();

        Ok(Alert {
            alert_level,
            alert_description,
        })
    }
}
