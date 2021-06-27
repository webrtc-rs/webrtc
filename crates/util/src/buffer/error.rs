use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum Error {
    // from buffer
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
