use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
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

    #[allow(non_camel_case_types)]
    #[error("{0}")]
    new(String),
}

impl Error {
    pub fn equal(&self, err: &anyhow::Error) -> bool {
        err.downcast_ref::<Self>().map_or(false, |e| e == self)
    }
}
