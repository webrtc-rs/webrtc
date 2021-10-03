use anyhow::Result;

use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum Error {
    #[error("Invalid Parent RTCP Reader")]
    ErrInvalidParentRtcpReader,
    #[error("Incorrect ReceiverReport CloseRx")]
    ErrIncorrectReceiverReportCloseRx,

    #[allow(non_camel_case_types)]
    #[error("{0}")]
    new(String),
}

impl Error {
    pub fn equal(&self, err: &anyhow::Error) -> bool {
        err.downcast_ref::<Self>().map_or(false, |e| e == self)
    }
}

/// flatten_errs flattens multiple errors into one
pub fn flatten_errs(errs: Vec<anyhow::Error>) -> Result<()> {
    if errs.is_empty() {
        Ok(())
    } else {
        let errs_strs: Vec<String> = errs.into_iter().map(|e| e.to_string()).collect();
        Err(Error::new(errs_strs.join("\n")).into())
    }
}
