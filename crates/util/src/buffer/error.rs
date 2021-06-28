use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum Error {
    #[error("buffer: full")]
    ErrBufferFull,
    #[error("buffer: closed")]
    ErrBufferClosed,
    #[error("buffer: short")]
    ErrBufferShort,
    #[error("packet too big")]
    ErrPacketTooBig,
    #[error("i/o timeout")]
    ErrTimeout,

    #[allow(non_camel_case_types)]
    #[error("{0}")]
    new(String),
}

impl Error {
    pub fn equal(&self, err: &anyhow::Error) -> bool {
        err.downcast_ref::<Self>().map_or(false, |e| e == self)
    }
}
