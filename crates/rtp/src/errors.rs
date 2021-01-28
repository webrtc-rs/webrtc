#[derive(Debug, PartialEq)]
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
        write!(f, "()")
    }
}

impl std::error::Error for RTPError {}

#[derive(Debug, PartialEq)]
pub enum ExtensionError {
    TooSmall,
    AudioLevelOverflow,
}
