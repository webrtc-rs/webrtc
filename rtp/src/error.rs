use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug, PartialEq)]
#[non_exhaustive]
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

    #[error("corrupted h265 packet")]
    ErrH265CorruptedPacket,
    #[error("invalid h265 packet type")]
    ErrInvalidH265PacketType,

    #[error("payload is too small for OBU extension header")]
    ErrPayloadTooSmallForObuExtensionHeader,
    #[error("payload is too small for OBU payload size")]
    ErrPayloadTooSmallForObuPayloadSize,

    #[error("extension_payload must be in 32-bit words")]
    HeaderExtensionPayloadNot32BitWords,
    #[error("audio level overflow")]
    AudioLevelOverflow,
    #[error("playout delay overflow")]
    PlayoutDelayOverflow,
    #[error("payload is not large enough")]
    PayloadIsNotLargeEnough,
    #[error("STAP-A declared size({0}) is larger than buffer({1})")]
    StapASizeLargerThanBuffer(usize, usize),
    #[error("nalu type {0} is currently not handled")]
    NaluTypeIsNotHandled(u8),
    #[error("{0}")]
    Util(#[from] util::Error),

    #[error("{0}")]
    Other(String),
}

impl From<Error> for util::Error {
    fn from(e: Error) -> Self {
        util::Error::from_std(e)
    }
}

impl PartialEq<util::Error> for Error {
    fn eq(&self, other: &util::Error) -> bool {
        if let Some(down) = other.downcast_ref::<Error>() {
            self == down
        } else {
            false
        }
    }
}
