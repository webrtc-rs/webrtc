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

    #[error("Other errors:{0}")]
    ErrOthers(String),
}

impl Error {
    pub fn equal(&self, err: &anyhow::Error) -> bool {
        if let Some(e) = err.downcast_ref::<Self>() {
            e == self
        } else {
            false
        }
    }
}
