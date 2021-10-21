use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug, PartialEq)]
#[non_exhaustive]
pub enum Error {
    #[error("Invalid Parent RTCP Reader")]
    ErrInvalidParentRtcpReader,
    #[error("Invalid Next RTP Writer")]
    ErrInvalidNextRtpWriter,
    #[error("Incorrect ReceiverReport CloseRx")]
    ErrIncorrectReceiverReportCloseRx,
    #[error("IO EOF")]
    ErrIoEOF,
    #[error("Buffer is too short")]
    ErrShortBuffer,
    /// ErrInvalidSize is returned by newReceiveLog/newSendBuffer, when an incorrect buffer size is supplied.
    #[error("invalid buffer size")]
    ErrInvalidSize,

    #[error("{0}")]
    Srtp(#[from] srtp::Error),
    #[error("{0}")]
    Rtcp(#[from] rtcp::Error),
    #[error("{0}")]
    Rtp(#[from] rtp::Error),
    #[error("{0}")]
    Util(#[from] util::Error),

    #[error("{0}")]
    Other(String),
}

/// flatten_errs flattens multiple errors into one
pub fn flatten_errs(errs: Vec<Error>) -> Result<()> {
    if errs.is_empty() {
        Ok(())
    } else {
        let errs_strs: Vec<String> = errs.into_iter().map(|e| e.to_string()).collect();
        Err(Error::Other(errs_strs.join("\n")))
    }
}
