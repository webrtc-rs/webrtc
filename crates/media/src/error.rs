use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
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

    #[allow(non_camel_case_types)]
    #[error("{0}")]
    new(String),
}

impl Error {
    pub fn equal(&self, err: &anyhow::Error) -> bool {
        err.downcast_ref::<Self>().map_or(false, |e| e == self)
    }
}
