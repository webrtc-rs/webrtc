use thiserror::Error;

mod stream;

pub use stream::{Error as StreamError, Stream};

pub mod association {
    use super::*;

    use crate::sctp::{PayloadType, Stream};

    #[derive(Error, Eq, PartialEq, Clone, Debug)]
    pub enum Error {}

    #[derive(Debug)]
    pub struct Association;

    impl Association {
        pub fn open_stream(&self, _id: u16, _payload_type: PayloadType) -> Result<Stream, Error> {
            todo!()
        }

        pub fn accept_stream(&self) -> Result<Stream, Error> {
            todo!()
        }
    }
}

pub use association::{Association, Error as AssociationError};

#[derive(Eq, PartialEq, Copy, Clone, Debug)]
pub enum PayloadType {
    WebRtcBinary,
    WebRtcBinaryEmpty,
    WebRtcDcep,
    WebRtcString,
    WebRtcStringEmpty,
}

impl PayloadType {
    pub fn is_empty(&self) -> bool {
        match self {
            PayloadType::WebRtcBinary => false,
            PayloadType::WebRtcBinaryEmpty => true,
            PayloadType::WebRtcDcep => false,
            PayloadType::WebRtcString => false,
            PayloadType::WebRtcStringEmpty => true,
        }
    }

    pub fn is_binary(&self) -> bool {
        match self {
            PayloadType::WebRtcBinary => true,
            PayloadType::WebRtcBinaryEmpty => true,
            PayloadType::WebRtcDcep => false,
            PayloadType::WebRtcString => false,
            PayloadType::WebRtcStringEmpty => false,
        }
    }

    pub fn is_string(&self) -> bool {
        match self {
            PayloadType::WebRtcBinary => false,
            PayloadType::WebRtcBinaryEmpty => false,
            PayloadType::WebRtcDcep => false,
            PayloadType::WebRtcString => true,
            PayloadType::WebRtcStringEmpty => true,
        }
    }
}

#[derive(Eq, PartialEq, Copy, Clone, Debug)]
pub enum ReliabilityType {
    Reliable,
    Rexmit,
    Timed,
}
