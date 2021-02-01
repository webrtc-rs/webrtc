#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RTPError {
    HeaderSizeInsufficient,
    HeaderSizeInsufficientForExtension,
    RFC8285OneByteHeaderIDRange(u8),
    RFC8285OneByteHeaderSize(u8),
    RFC8285TwoByteHeaderIDRange(u8),
    RFC8285TwoByteHeaderSize(u8),
    RFC3550HeaderIDRange(u8),
    ShortPacket,
    UnhandledNALUType(u8),
    ExtensionError(ExtensionError),
    ShortBuffer,
    HeaderExtensionNotEnabled,
    HeaderExtensionNotFound,
}

impl std::fmt::Display for RTPError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            RTPError::HeaderSizeInsufficient => write!(f, "RTP header size insufficient"),
            RTPError::HeaderSizeInsufficientForExtension => {
                write!(f, "RTP header size insufficient for extension")
            }
            RTPError::RFC8285OneByteHeaderIDRange(ref e) => write!(
                f,
                "header extension id must be between 1 and 14 for RFC 5285 one byte extensions: {}",e
            ),
            RTPError::RFC8285OneByteHeaderSize(ref e) => write!(
                f,
                "header extension payload must be 16bytes or less for RFC 5285 one byte extensions: {}"
            ,e),
            RTPError::RFC8285TwoByteHeaderIDRange(ref e) => write!(f, "header extension id must be between 1 and 255 for RFC 5285 two byte extensions: {}",e),
            RTPError::RFC8285TwoByteHeaderSize(ref e) => write!(f, "header extension payload must be 255bytes or less for RFC 5285 two byte extensions: {}",e),
            RTPError::RFC3550HeaderIDRange(ref e) => write!(f, "header extension id must be 0 for non-RFC 5285 extensions: {}",e),
            RTPError::ShortPacket => write!(f, "packet is not large enough"),
            RTPError::UnhandledNALUType(ref e) => write!(f, "NALU Type is unhandled: {}",e),
            RTPError::ExtensionError(ref e) => write!(f, "extension error: {}",e),
            RTPError::ShortBuffer => write!(f, "buffer too small"),
            RTPError::HeaderExtensionNotEnabled => write!(f, "header extension is not enabled"),
            RTPError::HeaderExtensionNotFound => write!(f, "header extension not found"),
        }
    }
}

impl std::error::Error for RTPError {}

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum ExtensionError {
    TooSmall,
    AudioLevelOverflow,
}

impl std::fmt::Display for ExtensionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            ExtensionError::AudioLevelOverflow => write!(f, "audio level overflow"),
            ExtensionError::TooSmall => write!(f, "buffer too small"),
        }
    }
}

impl std::error::Error for ExtensionError {}
