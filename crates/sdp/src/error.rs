use std::{num::ParseIntError, string::FromUtf8Error};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("codec not found")]
    CodecNotFound,
    #[error("could not extract codec from rtcp-fb")]
    RtcpFb,
    #[error("could not extract codec from fmtp")]
    FmtpParse,
    #[error("could not extract codec from rtpmap")]
    RtpmapParse,
    #[error("payload type not found")]
    PayloadTypeNotFound,
    #[error("SyntaxError: {0}")]
    ExtMapParse(String),
    #[error("sdp: empty time_descriptions")]
    SdpEmptyTimeDescription,
    #[error("SdpInvalidSyntax: {0}")]
    SdpInvalidSyntax(String),
    #[error("SdpInvalidValue: {0}")]
    SdpInvalidValue(String),
    #[error("FromUtf8Error: {0}")]
    Utf8Error(#[from] FromUtf8Error),
    #[error("ParseIntError: {0}")]
    ParseIntError(#[from] ParseIntError),
    #[error("UrlParseError: {0}")]
    UrlParseError(#[from] url::ParseError),
    #[error("IoError: {0}")]
    Io(#[from] std::io::Error),
}
