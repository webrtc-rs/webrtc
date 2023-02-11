use std::string::FromUtf8Error;
use std::{io, net};

use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error, PartialEq)]
#[non_exhaustive]
pub enum Error {
    #[error("mDNS: failed to join multicast group")]
    ErrJoiningMulticastGroup,
    #[error("mDNS: connection is closed")]
    ErrConnectionClosed,
    #[error("mDNS: context has elapsed")]
    ErrContextElapsed,
    #[error("mDNS: config must not be nil")]
    ErrNilConfig,
    #[error("parsing/packing of this type isn't available yet")]
    ErrNotStarted,
    #[error("parsing/packing of this section has completed")]
    ErrSectionDone,
    #[error("parsing/packing of this section is header")]
    ErrSectionHeader,
    #[error("insufficient data for base length type")]
    ErrBaseLen,
    #[error("insufficient data for calculated length type")]
    ErrCalcLen,
    #[error("segment prefix is reserved")]
    ErrReserved,
    #[error("too many pointers (>10)")]
    ErrTooManyPtr,
    #[error("invalid pointer")]
    ErrInvalidPtr,
    #[error("nil resource body")]
    ErrNilResourceBody,
    #[error("insufficient data for resource body length")]
    ErrResourceLen,
    #[error("segment length too long")]
    ErrSegTooLong,
    #[error("zero length segment")]
    ErrZeroSegLen,
    #[error("resource length too long")]
    ErrResTooLong,
    #[error("too many Questions to pack (>65535)")]
    ErrTooManyQuestions,
    #[error("too many Answers to pack (>65535)")]
    ErrTooManyAnswers,
    #[error("too many Authorities to pack (>65535)")]
    ErrTooManyAuthorities,
    #[error("too many Additionals to pack (>65535)")]
    ErrTooManyAdditionals,
    #[error("name is not in canonical format (it must end with a .)")]
    ErrNonCanonicalName,
    #[error("character string exceeds maximum length (255)")]
    ErrStringTooLong,
    #[error("compressed name in SRV resource data")]
    ErrCompressedSrv,
    #[error("empty builder msg")]
    ErrEmptyBuilderMsg,
    #[error("{0}")]
    Io(#[source] IoError),
    #[error("utf-8 error: {0}")]
    Utf8(#[from] FromUtf8Error),
    #[error("parse addr: {0}")]
    ParseIp(#[from] net::AddrParseError),
    #[error("{0}")]
    Other(String),
}

#[derive(Debug, Error)]
#[error("io error: {0}")]
pub struct IoError(#[from] pub io::Error);

// Workaround for wanting PartialEq for io::Error.
impl PartialEq for IoError {
    fn eq(&self, other: &Self) -> bool {
        self.0.kind() == other.0.kind()
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(IoError(e))
    }
}
