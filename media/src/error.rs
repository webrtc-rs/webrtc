use std::io;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug, PartialEq)]
#[non_exhaustive]
pub enum Error {
    #[error("stream is nil")]
    ErrNilStream,
    #[error("incomplete frame header")]
    ErrIncompleteFrameHeader,
    #[error("incomplete frame data")]
    ErrIncompleteFrameData,
    #[error("incomplete file header")]
    ErrIncompleteFileHeader,
    #[error("IVF signature mismatch")]
    ErrSignatureMismatch,
    #[error("IVF version unknown, parser may not parse correctly")]
    ErrUnknownIVFVersion,

    #[error("file not opened")]
    ErrFileNotOpened,
    #[error("invalid nil packet")]
    ErrInvalidNilPacket,

    #[error("bad header signature")]
    ErrBadIDPageSignature,
    #[error("wrong header, expected beginning of stream")]
    ErrBadIDPageType,
    #[error("payload for id page must be 19 bytes")]
    ErrBadIDPageLength,
    #[error("bad payload signature")]
    ErrBadIDPagePayloadSignature,
    #[error("not enough data for payload header")]
    ErrShortPageHeader,
    #[error("expected and actual checksum do not match")]
    ErrChecksumMismatch,

    #[error("data is not a H264 bitstream")]
    ErrDataIsNotH264Stream,
    #[error("Io EOF")]
    ErrIoEOF,

    #[allow(non_camel_case_types)]
    #[error("{0}")]
    Io(#[source] IoError),
    #[error("{0}")]
    Rtp(#[from] rtp::Error),

    #[error("{0}")]
    Other(String),
}

#[derive(Debug, Error)]
#[error("io error: {0}")]
pub struct IoError(#[from] pub io::Error);

// Workaround for wanting PartialEq for io::Error.
impl PartialEq for IoError {
    fn eq(&self, other: &Self) -> bool {
        self.0.kind() == other.0.kind()
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(IoError(e))
    }
}
