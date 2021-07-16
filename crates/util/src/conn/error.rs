use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum Error {
    #[error("udp: listener closed")]
    ErrClosedListener,
    #[error("udp: listen queue exceeded")]
    ErrListenQueueExceeded,
    #[error("udp: listener accept ch closed")]
    ErrClosedListenerAcceptCh,

    #[allow(non_camel_case_types)]
    #[error("{0}")]
    new(String),
}

impl Error {
    pub fn equal(&self, err: &anyhow::Error) -> bool {
        err.downcast_ref::<Self>().map_or(false, |e| e == self)
    }
}
