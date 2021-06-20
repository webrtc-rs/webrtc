use crate::stream::Stream;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("duplicated packet")]
    ErrDuplicated,
    #[error("SRTP master key is not long enough")]
    ErrShortSrtpMasterKey,
    #[error("SRTP master salt is not long enough")]
    ErrShortSrtpMasterSalt,
    #[error("no such SRTP Profile")]
    ErrNoSuchSrtpProfile,
    #[error("indexOverKdr > 0 is not supported yet")]
    ErrNonZeroKdrNotSupported,
    #[error("exporter called with wrong label")]
    ErrExporterWrongLabel,
    #[error("no config provided")]
    ErrNoConfig,
    #[error("no conn provided")]
    ErrNoConn,
    #[error("failed to verify auth tag")]
    ErrFailedToVerifyAuthTag,
    #[error("packet is too short to be rtcp packet")]
    ErrTooShortRtcp,
    #[error("payload differs")]
    ErrPayloadDiffers,
    #[error("started channel used incorrectly, should only be closed")]
    ErrStartedChannelUsedIncorrectly,
    #[error("stream has not been inited, unable to close")]
    ErrStreamNotInited,
    #[error("stream is already closed")]
    ErrStreamAlreadyClosed,
    #[error("stream is already inited")]
    ErrStreamAlreadyInited,
    #[error("failed to cast child")]
    ErrFailedTypeAssertion,

    #[error("index_over_kdr > 0 is not supported yet")]
    UnsupportedIndexOverKdr,
    #[error("SRTP Master Key must be len {0}, got {1}")]
    SrtpMasterKeyLength(usize, usize),
    #[error("SRTP Salt must be len {0}, got {1}")]
    SrtpSaltLength(usize, usize),
    #[error("SyntaxError: {0}")]
    ExtMapParse(String),
    #[error("ssrc {0} not exist in srtp_ssrc_state")]
    SsrcMissingFromSrtp(u32),
    #[error("srtp ssrc={0} index={1}: duplicated")]
    SrtpSsrcDuplicated(u32, u16),
    #[error("srtcp ssrc={0} index={1}: duplicated")]
    SrtcpSsrcDuplicated(u32, usize),
    #[error("ssrc {0} not exist in srtcp_ssrc_state")]
    SsrcMissingFromSrtcp(u32),
    #[error("Stream with ssrc {0} exists")]
    StreamWithSsrcExists(u32),
    #[error("Session RTP/RTCP type must be same as input buffer")]
    SessionRtpRtcpTypeMismatch,
    #[error("Session EOF")]
    SessionEof,
    #[error("too short SRTP packet: only {0} bytes, expected > {1} bytes")]
    SrtpTooSmall(usize, usize),
    #[error("too short SRTCP packet: only {0} bytes, expected > {1} bytes")]
    SrtcpTooSmall(usize, usize),
    #[error("failed to verify rtp auth tag")]
    RtpFailedToVerifyAuthTag,
    #[error("failed to verify rtcp auth tag")]
    RtcpFailedToVerifyAuthTag,
    #[error("SessionSRTP has been closed")]
    SessionSrtpAlreadyClosed,
    #[error("this stream is not a RTPStream")]
    InvalidRtpStream,
    #[error("this stream is not a RTCPStream")]
    InvalidRtcpStream,
    #[error("UtilError: {0}")]
    UtilError(#[from] util::error::Error),
    #[error("RtpError: {0}")]
    RtpError(#[from] rtp::error::Error),
    #[error("RtcpError: {0}")]
    RtcpError(#[from] rtcp::error::Error),
    #[error("IoError: {0}")]
    Io(#[from] std::io::Error),
    #[error("AesGcm: {0}")]
    AesGcm(#[from] aes_gcm::Error),
    #[error("InvalidKeyLength: {0}")]
    InvalidKeyLength(#[from] hmac::crypto_mac::InvalidKeyLength),
    #[error("SendError: {0}")]
    SendUnit(#[from] tokio::sync::mpsc::error::SendError<()>),
    #[error("SendError: {0}")]
    SendU32(#[from] tokio::sync::mpsc::error::SendError<u32>),
    #[error("SendError: {0}")]
    SendStream(#[from] tokio::sync::mpsc::error::SendError<Stream>),

    #[error("Other errors: {0}")]
    ErrOthers(String),
}
