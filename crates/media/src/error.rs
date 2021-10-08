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
    new(String),
}

impl Error {
    pub fn equal(&self, err: &anyhow::Error) -> bool {
        err.downcast_ref::<Self>().map_or(false, |e| e == self)
    }
}
