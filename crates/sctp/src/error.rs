use thiserror::Error;

#[derive(Debug, Error)]
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

    #[error("unhandled ParamType")]
    ErrParamTypeUnhandled,

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
    ErrChunkTypeInitInitateTagZero,
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
    #[error("SACK Chunk size does not match predicted amount from header values")]
    ErrSackSizeNotMatchPredicted,

    #[error("invalid chunk size")]
    ErrInvalidChunkSize,
    #[error("ChunkType is not of type SHUTDOWN")]
    ErrChunkTypeNotShutdown,

    #[error("ChunkType is not of type SHUTDOWN-ACK")]
    ErrChunkTypeNotShutdownAck,
}
