use std::io;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub enum Error {
    #[error("raw is too small for a SCTP chunk")]
    ErrChunkHeaderTooSmall,
    #[error("not enough data left in SCTP packet to satisfy requested length")]
    ErrChunkHeaderNotEnoughSpace,
    #[error("chunk PADDING is non-zero at offset")]
    ErrChunkHeaderPaddingNonZero,
    #[error("chunk has invalid length")]
    ErrChunkHeaderInvalidLength,

    #[error("ChunkType is not of type ABORT")]
    ErrChunkTypeNotAbort,
    #[error("failed build Abort Chunk")]
    ErrBuildAbortChunkFailed,
    #[error("ChunkType is not of type COOKIEACK")]
    ErrChunkTypeNotCookieAck,
    #[error("ChunkType is not of type COOKIEECHO")]
    ErrChunkTypeNotCookieEcho,
    #[error("ChunkType is not of type ctError")]
    ErrChunkTypeNotCtError,
    #[error("failed build Error Chunk")]
    ErrBuildErrorChunkFailed,
    #[error("failed to marshal stream")]
    ErrMarshalStreamFailed,
    #[error("chunk too short")]
    ErrChunkTooShort,
    #[error("ChunkType is not of type ForwardTsn")]
    ErrChunkTypeNotForwardTsn,
    #[error("ChunkType is not of type HEARTBEAT")]
    ErrChunkTypeNotHeartbeat,
    #[error("ChunkType is not of type HEARTBEATACK")]
    ErrChunkTypeNotHeartbeatAck,
    #[error("heartbeat is not long enough to contain Heartbeat Info")]
    ErrHeartbeatNotLongEnoughInfo,
    #[error("failed to parse param type")]
    ErrParseParamTypeFailed,
    #[error("heartbeat should only have HEARTBEAT param")]
    ErrHeartbeatParam,
    #[error("failed unmarshalling param in Heartbeat Chunk")]
    ErrHeartbeatChunkUnmarshal,
    #[error("unimplemented")]
    ErrUnimplemented,
    #[error("heartbeat Ack must have one param")]
    ErrHeartbeatAckParams,
    #[error("heartbeat Ack must have one param, and it should be a HeartbeatInfo")]
    ErrHeartbeatAckNotHeartbeatInfo,
    #[error("unable to marshal parameter for Heartbeat Ack")]
    ErrHeartbeatAckMarshalParam,

    #[error("raw is too small for error cause")]
    ErrErrorCauseTooSmall,

    #[error("unhandled ParamType `{typ}`")]
    ErrParamTypeUnhandled { typ: u16 },

    #[error("unexpected ParamType")]
    ErrParamTypeUnexpected,

    #[error("param header too short")]
    ErrParamHeaderTooShort,
    #[error("param self reported length is shorter than header length")]
    ErrParamHeaderSelfReportedLengthShorter,
    #[error("param self reported length is longer than header length")]
    ErrParamHeaderSelfReportedLengthLonger,
    #[error("failed to parse param type")]
    ErrParamHeaderParseFailed,

    #[error("packet to short")]
    ErrParamPacketTooShort,
    #[error("outgoing SSN reset request parameter too short")]
    ErrSsnResetRequestParamTooShort,
    #[error("reconfig response parameter too short")]
    ErrReconfigRespParamTooShort,
    #[error("invalid algorithm type")]
    ErrInvalidAlgorithmType,

    #[error("failed to parse param type")]
    ErrInitChunkParseParamTypeFailed,
    #[error("failed unmarshalling param in Init Chunk")]
    ErrInitChunkUnmarshalParam,
    #[error("unable to marshal parameter for INIT/INITACK")]
    ErrInitAckMarshalParam,

    #[error("ChunkType is not of type INIT")]
    ErrChunkTypeNotTypeInit,
    #[error("chunk Value isn't long enough for mandatory parameters exp")]
    ErrChunkValueNotLongEnough,
    #[error("ChunkType of type INIT flags must be all 0")]
    ErrChunkTypeInitFlagZero,
    #[error("failed to unmarshal INIT body")]
    ErrChunkTypeInitUnmarshalFailed,
    #[error("failed marshaling INIT common data")]
    ErrChunkTypeInitMarshalFailed,
    #[error("ChunkType of type INIT ACK InitiateTag must not be 0")]
    ErrChunkTypeInitInitiateTagZero,
    #[error("INIT ACK inbound stream request must be > 0")]
    ErrInitInboundStreamRequestZero,
    #[error("INIT ACK outbound stream request must be > 0")]
    ErrInitOutboundStreamRequestZero,
    #[error("INIT ACK Advertised Receiver Window Credit (a_rwnd) must be >= 1500")]
    ErrInitAdvertisedReceiver1500,

    #[error("packet is smaller than the header size")]
    ErrChunkPayloadSmall,
    #[error("ChunkType is not of type PayloadData")]
    ErrChunkTypeNotPayloadData,
    #[error("ChunkType is not of type Reconfig")]
    ErrChunkTypeNotReconfig,
    #[error("ChunkReconfig has invalid ParamA")]
    ErrChunkReconfigInvalidParamA,

    #[error("failed to parse param type")]
    ErrChunkParseParamTypeFailed,
    #[error("unable to marshal parameter A for reconfig")]
    ErrChunkMarshalParamAReconfigFailed,
    #[error("unable to marshal parameter B for reconfig")]
    ErrChunkMarshalParamBReconfigFailed,

    #[error("ChunkType is not of type SACK")]
    ErrChunkTypeNotSack,
    #[error("SACK Chunk size is not large enough to contain header")]
    ErrSackSizeNotLargeEnoughInfo,

    #[error("invalid chunk size")]
    ErrInvalidChunkSize,
    #[error("ChunkType is not of type SHUTDOWN")]
    ErrChunkTypeNotShutdown,

    #[error("ChunkType is not of type SHUTDOWN-ACK")]
    ErrChunkTypeNotShutdownAck,
    #[error("ChunkType is not of type SHUTDOWN-COMPLETE")]
    ErrChunkTypeNotShutdownComplete,

    #[error("raw is smaller than the minimum length for a SCTP packet")]
    ErrPacketRawTooSmall,
    #[error("unable to parse SCTP chunk, not enough data for complete header")]
    ErrParseSctpChunkNotEnoughData,
    #[error("failed to unmarshal, contains unknown chunk type")]
    ErrUnmarshalUnknownChunkType,
    #[error("checksum mismatch theirs")]
    ErrChecksumMismatch,

    #[error("unexpected chunk popped (unordered)")]
    ErrUnexpectedChuckPoppedUnordered,
    #[error("unexpected chunk popped (ordered)")]
    ErrUnexpectedChuckPoppedOrdered,
    #[error("unexpected q state (should've been selected)")]
    ErrUnexpectedQState,
    #[error("try again")]
    ErrTryAgain,

    #[error("abort chunk, with following errors")]
    ErrChunk,
    #[error("shutdown called in non-Established state")]
    ErrShutdownNonEstablished,
    #[error("association closed before connecting")]
    ErrAssociationClosedBeforeConn,
    #[error("association init failed")]
    ErrAssociationInitFailed,
    #[error("association handshake closed")]
    ErrAssociationHandshakeClosed,
    #[error("silently discard")]
    ErrSilentlyDiscard,
    #[error("the init not stored to send")]
    ErrInitNotStoredToSend,
    #[error("cookieEcho not stored to send")]
    ErrCookieEchoNotStoredToSend,
    #[error("sctp packet must not have a source port of 0")]
    ErrSctpPacketSourcePortZero,
    #[error("sctp packet must not have a destination port of 0")]
    ErrSctpPacketDestinationPortZero,
    #[error("init chunk must not be bundled with any other chunk")]
    ErrInitChunkBundled,
    #[error("init chunk expects a verification tag of 0 on the packet when out-of-the-blue")]
    ErrInitChunkVerifyTagNotZero,
    #[error("todo: handle Init when in state")]
    ErrHandleInitState,
    #[error("no cookie in InitAck")]
    ErrInitAckNoCookie,
    #[error("there already exists a stream with identifier")]
    ErrStreamAlreadyExist,
    #[error("Failed to create a stream with identifier")]
    ErrStreamCreateFailed,
    #[error("unable to be popped from inflight queue TSN")]
    ErrInflightQueueTsnPop,
    #[error("requested non-existent TSN")]
    ErrTsnRequestNotExist,
    #[error("sending reset packet in non-Established state")]
    ErrResetPacketInStateNotExist,
    #[error("unexpected parameter type")]
    ErrParameterType,
    #[error("sending payload data in non-Established state")]
    ErrPayloadDataStateNotExist,
    #[error("unhandled chunk type")]
    ErrChunkTypeUnhandled,
    #[error("handshake failed (INIT ACK)")]
    ErrHandshakeInitAck,
    #[error("handshake failed (COOKIE ECHO)")]
    ErrHandshakeCookieEcho,

    #[error("outbound packet larger than maximum message size")]
    ErrOutboundPacketTooLarge,
    #[error("Stream closed")]
    ErrStreamClosed,
    #[error("Short buffer (size: {size:?}) to be filled")]
    ErrShortBuffer { size: usize },
    #[error("Io EOF")]
    ErrEof,
    #[error("Invalid SystemTime")]
    ErrInvalidSystemTime,
    #[error("Net Conn read error")]
    ErrNetConnReadError,
    #[error("Max Data Channel ID")]
    ErrMaxDataChannelID,

    #[error("{0}")]
    Other(String),
}

impl From<Error> for io::Error {
    fn from(error: Error) -> Self {
        match error {
            e @ Error::ErrEof => io::Error::new(io::ErrorKind::UnexpectedEof, e.to_string()),
            e @ Error::ErrStreamClosed => {
                io::Error::new(io::ErrorKind::ConnectionAborted, e.to_string())
            }
            e => io::Error::new(io::ErrorKind::Other, e.to_string()),
        }
    }
}
