#[derive(Debug)]
pub enum RTPError {
    HeaderSizeInsufficient(String),
    HeaderSizeInsufficientForExtension(String),
    RFC8285OneByteHeaderIDRange(String),
    RFC8285OneByteHeaderSize(String),
    RFC8285TwoByteHeaderIDRange(String),
    RFC8285TwoByteHeaderSize(String),
    RFC3550HeaderIDRange(String),
    ShortPacket(String),
    UnhandledNALUType(String),
    ExtensionError(ExtensionError),
    ShortBuffer,
    HeaderExtensionNotEnabled,
    HeaderExtensionNotFound,
}

impl std::fmt::Display for RTPError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "()")
    }
}

impl std::error::Error for RTPError {}

#[derive(Debug)]
pub enum ExtensionError {
    TooSmall,
    AudioLevelOverflow,
}
