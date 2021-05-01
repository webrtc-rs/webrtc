use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("raw is too small for a SCTP chunk")]
    ErrChunkHeaderTooSmall,
    #[error("not enough data left in SCTP packet to satisfy requested length")]
    ErrChunkHeaderNotEnoughSpace,
    #[error("chunk padding is non-zero at offset")]
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

    #[error("raw is too small for error cause")]
    ErrErrorCauseTooSmall,
}
