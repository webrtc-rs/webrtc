use std::io;
use std::string::FromUtf8Error;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error, PartialEq)]
#[non_exhaustive]
pub enum Error {
    #[error(
        "DataChannel message is not long enough to determine type: (expected: {expected}, actual: {actual})"
    )]
    UnexpectedEndOfBuffer { expected: usize, actual: usize },
    #[error("Unknown MessageType {0}")]
    InvalidMessageType(u8),
    #[error("Unknown ChannelType {0}")]
    InvalidChannelType(u8),
    #[error("Unknown PayloadProtocolIdentifier {0}")]
    InvalidPayloadProtocolIdentifier(u8),
    #[error("Stream closed")]
    ErrStreamClosed,

    #[error("{0}")]
    Util(#[from] util::Error),
    #[error("{0}")]
    Sctp(#[from] sctp::Error),
    #[error("utf-8 error: {0}")]
    Utf8(#[from] FromUtf8Error),

    #[allow(non_camel_case_types)]
    #[error("{0}")]
    new(String),
}

impl From<Error> for util::Error {
    fn from(e: Error) -> Self {
        util::Error::from_std(e)
    }
}

impl From<Error> for io::Error {
    fn from(error: Error) -> Self {
        match error {
            e @ Error::Sctp(sctp::Error::ErrEof) => {
                io::Error::new(io::ErrorKind::UnexpectedEof, e.to_string())
            }
            e @ Error::ErrStreamClosed => {
                io::Error::new(io::ErrorKind::ConnectionAborted, e.to_string())
            }
            e => io::Error::new(io::ErrorKind::Other, e.to_string()),
        }
    }
}

impl PartialEq<util::Error> for Error {
    fn eq(&self, other: &util::Error) -> bool {
        if let Some(down) = other.downcast_ref::<Error>() {
            return self == down;
        }
        false
    }
}

impl PartialEq<Error> for util::Error {
    fn eq(&self, other: &Error) -> bool {
        if let Some(down) = self.downcast_ref::<Error>() {
            return other == down;
        }
        false
    }
}
