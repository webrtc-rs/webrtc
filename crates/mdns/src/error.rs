use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum Error {
    #[error("mDNS: failed to join multicast group")]
    ErrJoiningMulticastGroup,
    #[error("mDNS: connection is closed")]
    ErrConnectionClosed,
    #[error("mDNS: context has elapsed")]
    ErrContextElapsed,
    #[error("mDNS: config must not be nil")]
    ErrNilConfig,
    #[error("parsing/packing of this type isn't available yet")]
    ErrNotStarted,
    #[error("parsing/packing of this section has completed")]
    ErrSectionDone,
    #[error("parsing/packing of this section is header")]
    ErrSectionHeader,
    #[error("insufficient data for base length type")]
    ErrBaseLen,
    #[error("insufficient data for calculated length type")]
    ErrCalcLen,
    #[error("segment prefix is reserved")]
    ErrReserved,
    #[error("too many pointers (>10)")]
    ErrTooManyPtr,
    #[error("invalid pointer")]
    ErrInvalidPtr,
    #[error("nil resource body")]
    ErrNilResourceBody,
    #[error("insufficient data for resource body length")]
    ErrResourceLen,
    #[error("segment length too long")]
    ErrSegTooLong,
    #[error("zero length segment")]
    ErrZeroSegLen,
    #[error("resource length too long")]
    ErrResTooLong,
    #[error("too many Questions to pack (>65535)")]
    ErrTooManyQuestions,
    #[error("too many Answers to pack (>65535)")]
    ErrTooManyAnswers,
    #[error("too many Authorities to pack (>65535)")]
    ErrTooManyAuthorities,
    #[error("too many Additionals to pack (>65535)")]
    ErrTooManyAdditionals,
    #[error("name is not in canonical format (it must end with a .)")]
    ErrNonCanonicalName,
    #[error("character string exceeds maximum length (255)")]
    ErrStringTooLong,
    #[error("compressed name in SRV resource data")]
    ErrCompressedSrv,
    #[error("empty builder msg")]
    ErrEmptyBuilderMsg,

    #[error("Other errors:{0}")]
    ErrOthers(String),
}

impl Error {
    pub fn equal(&self, err: &anyhow::Error) -> bool {
        if let Some(e) = err.downcast_ref::<Self>() {
            e == self
        } else {
            false
        }
    }
}
