mod stream;

pub use stream::{Error as StreamError, Stream};

pub mod association {
    use crate::sctp::{PayloadType, Stream};

    #[derive(Eq, PartialEq, Clone, Debug)]
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
    WebRTCBinary,
    WebRTCBinaryEmpty,
    WebRTCDCEP,
    WebRTCString,
    WebRTCStringEmpty,
}

impl PayloadType {
    pub fn is_empty(&self) -> bool {
        match self {
            PayloadType::WebRTCBinary => false,
            PayloadType::WebRTCBinaryEmpty => true,
            PayloadType::WebRTCDCEP => false,
            PayloadType::WebRTCString => false,
            PayloadType::WebRTCStringEmpty => true,
        }
    }

    pub fn is_binary(&self) -> bool {
        match self {
            PayloadType::WebRTCBinary => true,
            PayloadType::WebRTCBinaryEmpty => true,
            PayloadType::WebRTCDCEP => false,
            PayloadType::WebRTCString => false,
            PayloadType::WebRTCStringEmpty => false,
        }
    }

    pub fn is_string(&self) -> bool {
        match self {
            PayloadType::WebRTCBinary => false,
            PayloadType::WebRTCBinaryEmpty => false,
            PayloadType::WebRTCDCEP => false,
            PayloadType::WebRTCString => true,
            PayloadType::WebRTCStringEmpty => true,
        }
    }
}

#[derive(Eq, PartialEq, Copy, Clone, Debug)]
pub enum ReliabilityType {
    Reliable,
    Rexmit,
    Timed,
}
