/// Possible RTCP error.
#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    /// Wrong marshal size.
    WrongMarshalSize,
    /// Packet lost exceeds maximum amount of packets
    /// that can possibly be lost.
    InvalidTotalLost,
    /// Packet contains an invalid header.
    InvalidHeader,
    /// Packet contains empty compound.
    EmptyCompound,
    /// Invalid first packet in compound packets. First packet
    /// should either be a SenderReport packet or ReceiverReport
    BadFirstPacket,
    /// CNAME was not defined.
    MissingCNAME,
    /// Packet was defined before CNAME.
    PacketBeforeCNAME,
    /// Too many reports.
    TooManyReports,
    /// Too many chunks.
    TooManyChunks,
    /// Too many sources.
    TooManySources,
    /// Packet received is too short.
    PacketTooShort,
    /// Wrong packet type.
    WrongType,
    /// SDES received is too long.
    SDESTextTooLong,
    /// SDES type is missing.
    SDESMissingType,
    /// Reason is too long.
    ReasonTooLong,
    /// Invalid packet version.
    BadVersion(String),
    /// Invalid padding value.
    WrongPadding(String),
    /// Wrong feedback message type.
    WrongFeedbackType(String),
    /// Wrong payload type.
    WrongPayloadType(String),
    /// Header length is too small.
    HeaderTooSmall,
    /// Media ssrc was defined as zero.
    SSRCMustBeZero,
    /// Missing REMB identifier.
    MissingREMBIdentifier,
    /// SSRC number and length mismatches.
    SSRCNumAndLengthMismatch,
    /// Invalid size or start index.
    InvalidSizeOrStartIndex,
    /// Delta exceeds limit.
    DeltaExceedLimit,
    /// Packet status chunk is not 2 bytes.
    PacketStatusChunkLength,
    /// Other undefined error.
    Other(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Error::BadFirstPacket => write!(f, "First packet in compound must be SR or RR"),
            Error::BadVersion(ref e) => match e.is_empty() {
                true => write!(f, "Invalid packet version"),
                _ => write!(f, "Invalid packet version, {}", e),
            },
            Error::DeltaExceedLimit => write!(f, "Delta exceed limit"),
            Error::EmptyCompound => write!(f, "Empty compound packet"),
            Error::HeaderTooSmall => write!(f, "Header length is too small"),
            Error::InvalidHeader => write!(f, "Invalid header"),
            Error::InvalidSizeOrStartIndex => {
                write!(f, "Invalid size or startIndex")
            }
            Error::InvalidTotalLost => {
                write!(f, "Invalid total lost count")
            }
            Error::MissingCNAME => write!(f, "Compound missing SourceDescription with CNAME"),
            Error::MissingREMBIdentifier => write!(f, "Missing REMB identifier"),
            Error::PacketBeforeCNAME => write!(f, "Feedback packet seen before CNAME"),
            Error::PacketStatusChunkLength => {
                write!(f, "Packet status chunk must be 2 bytes")
            }
            Error::PacketTooShort => write!(f, "Packet status chunk must be 2 bytes"),
            Error::ReasonTooLong => write!(f, "Reason must be < 255 octets long"),
            Error::SDESMissingType => write!(f, "SDES item missing type"),
            Error::SDESTextTooLong => write!(f, "SDES must be < 255 octets long"),
            Error::SSRCMustBeZero => write!(f, "Media SSRC must be 0"),
            Error::SSRCNumAndLengthMismatch => {
                write!(f, "SSRC num and length do not match")
            }
            Error::TooManyChunks => write!(f, "Too many chunks"),
            Error::TooManyReports => write!(f, "Too many reports"),
            Error::TooManySources => write!(f, "too many sources"),
            Error::WrongFeedbackType(ref e) => match e.is_empty() {
                true => write!(f, "Wrong feedback message type"),
                _ => write!(f, "Wrong feedback message type, {}", e),
            },
            Error::WrongMarshalSize => write!(f, "Wrong marshal size"),
            Error::WrongPadding(ref e) => match e.is_empty() {
                false => write!(f, "Invalid padding value, {}", e),
                _ => write!(f, "Invalid padding value"),
            },
            Error::WrongPayloadType(ref e) => match e.is_empty() {
                false => write!(f, "Wrong payload type, {}", e),
                _ => write!(f, "Wrong payload type"),
            },
            Error::WrongType => write!(f, "Wrong packet type"),
            Error::Other(ref e) => write!(f, "{}", e),
        }
    }
}
