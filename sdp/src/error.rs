use std::io;
use std::num::ParseIntError;
use std::string::FromUtf8Error;

use substring::Substring;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error, PartialEq)]
#[non_exhaustive]
pub enum Error {
    #[error("codec not found")]
    CodecNotFound,
    #[error("missing whitespace")]
    MissingWhitespace,
    #[error("missing colon")]
    MissingColon,
    #[error("payload type not found")]
    PayloadTypeNotFound,
    #[error("{0}")]
    Io(#[source] IoError),
    #[error("utf-8 error: {0}")]
    Utf8(#[from] FromUtf8Error),
    #[error("SdpInvalidSyntax: {0}")]
    SdpInvalidSyntax(String),
    #[error("SdpInvalidValue: {0}")]
    SdpInvalidValue(String),
    #[error("sdp: empty time_descriptions")]
    SdpEmptyTimeDescription,
    #[error("parse int: {0}")]
    ParseInt(#[from] ParseIntError),
    #[error("parse url: {0}")]
    ParseUrl(#[from] url::ParseError),
    #[error("parse extmap: {0}")]
    ParseExtMap(String),
    #[error("{} --> {} <-- {}", .s.substring(0,*.p), .s.substring(*.p, *.p+1), .s.substring(*.p+1, .s.len()))]
    SyntaxError { s: String, p: usize },
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
