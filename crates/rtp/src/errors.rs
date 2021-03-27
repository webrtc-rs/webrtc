use thiserror::Error;

#[derive(Debug, Clone, Error, Copy, PartialEq)]
pub enum RTPError {
    #[error("RTP header size insufficient")]
    HeaderSizeInsufficient,
    #[error("RTP header size insufficient for extension")]
    HeaderSizeInsufficientForExtension,
    #[error("header extension id must be between 1 and 14 for RFC 5285 one byte extensions: {0}")]
    RFC8285OneByteHeaderIDRange(u8),
    #[error(
        "header extension payload must be 16bytes or less for RFC 5285 one byte extensions: {0}"
    )]
    RFC8285OneByteHeaderSize(u8),
    #[error("header extension id must be between 1 and 255 for RFC 5285 two byte extensions: {0}")]
    RFC8285TwoByteHeaderIDRange(u8),
    #[error(
        "header extension payload must be 255bytes or less for RFC 5285 two byte extensions: {0}"
    )]
    RFC8285TwoByteHeaderSize(u8),
    #[error("header extension id must be 0 for non-RFC 5285 extensions: {0}")]
    RFC3550HeaderIDRange(u8),
    #[error("packet is not large enough")]
    ShortPacket,
    #[error("NALU Type is unhandled: {0}")]
    UnhandledNALUType(u8),
    #[error("extension error: {0}")]
    ExtensionError(ExtensionError),
    #[error("buffer too small")]
    ShortBuffer,
    #[error("header extension is not enabled")]
    HeaderExtensionNotEnabled,
    #[error("header extension not found")]
    HeaderExtensionNotFound,
}

#[derive(Debug, PartialEq, Error, Copy, Clone)]
pub enum ExtensionError {
    #[error("audio level overflow")]
    TooSmall,
    #[error("buffer too small")]
    AudioLevelOverflow,
}
