use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("header extension id must be between 1 and 14 for RFC 5285 extensions")]
    HeaderExtensionIdOneByteLength,
    #[error("header extension payload must be 16bytes or less for RFC 5285 one byte extensions")]
    HeaderExtensionPayloadOneByteLength,
    #[error("header extension id must be between 1 and 255 for RFC 5285 extensions")]
    HeaderExtensionIdTwoByteLength,
    #[error("header extension payload must be 255bytes or less for RFC 5285 two byte extensions")]
    HeaderExtensionPayloadTwoByteLength,
    #[error("header extension id must be 0 for none RFC 5285 extensions")]
    HeaderExtensionIdShouldBeZero,
    #[error("extension not enabled")]
    HeaderExtensionNotEnabled,
    #[error("extension not found")]
    HeaderExtensionNotFound,
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
    #[error("SystemTimeError: {0}")]
    SystemTime(#[from] std::time::SystemTimeError),
    #[error("IoError: {0}")]
    Io(#[from] std::io::Error),
}
