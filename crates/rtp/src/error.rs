use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum Error {
    #[error("RTP header size insufficient")]
    ErrHeaderSizeInsufficient,
    #[error("RTP header size insufficient for extension")]
    ErrHeaderSizeInsufficientForExtension,
    #[error("buffer too small")]
    ErrBufferTooSmall,
    #[error("extension not enabled")]
    ErrHeaderExtensionsNotEnabled,
    #[error("extension not found")]
    ErrHeaderExtensionNotFound,

    #[error("header extension id must be between 1 and 14 for RFC 5285 extensions")]
    ErrRfc8285oneByteHeaderIdrange,
    #[error("header extension payload must be 16bytes or less for RFC 5285 one byte extensions")]
    ErrRfc8285oneByteHeaderSize,

    #[error("header extension id must be between 1 and 255 for RFC 5285 extensions")]
    ErrRfc8285twoByteHeaderIdrange,
    #[error("header extension payload must be 255bytes or less for RFC 5285 two byte extensions")]
    ErrRfc8285twoByteHeaderSize,

    #[error("header extension id must be 0 for none RFC 5285 extensions")]
    ErrRfc3550headerIdrange,

    #[error("packet is not large enough")]
    ErrShortPacket,
    #[error("invalid nil packet")]
    ErrNilPacket,
    #[error("too many PDiff")]
    ErrTooManyPDiff,
    #[error("too many spatial layers")]
    ErrTooManySpatialLayers,
    #[error("NALU Type is unhandled")]
    ErrUnhandledNaluType,

    #[error("extension_payload must be in 32-bit words")]
    HeaderExtensionPayloadNot32BitWords,
    #[error("audio level overflow")]
    AudioLevelOverflow,
    #[error("payload is not large enough")]
    PayloadIsNotLargeEnough,
    #[error("STAP-A declared size({0}) is larger than buffer({1})")]
    StapASizeLargerThanBuffer(usize, usize),
    #[error("nalu type {0} is currently not handled")]
    NaluTypeIsNotHandled(u8),

    #[allow(non_camel_case_types)]
    #[error("{0}")]
    new(String),
}

impl Error {
    pub fn equal(&self, err: &anyhow::Error) -> bool {
        err.downcast_ref::<Self>().map_or(false, |e| e == self)
    }
}
