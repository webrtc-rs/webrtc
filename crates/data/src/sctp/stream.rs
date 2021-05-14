use bytes::{Buf, BytesMut};

use crate::sctp::{PayloadType, ReliabilityType};

#[derive(Eq, PartialEq, Clone, Debug)]
pub enum Error {
    Eof,
}

#[derive(Debug)]
pub struct Stream;

impl Stream {
    pub fn stream_identifier(&self) -> u16 {
        todo!()
    }

    pub fn read_sctp(&mut self, _bytes: &mut BytesMut) -> Result<(usize, PayloadType), Error> {
        todo!()
    }

    pub fn write_sctp<B>(
        &mut self,
        _bytes: &mut B,
        _payload_type: PayloadType,
    ) -> Result<usize, Error>
    where
        B: Buf,
    {
        todo!()
    }

    pub fn close(&mut self) -> Result<(), Error> {
        todo!()
    }

    pub fn buffered_amount(&self) -> u64 {
        todo!()
    }

    pub fn buffered_amount_low_threshold(&self) -> u64 {
        todo!()
    }

    pub fn set_buffered_amount_low_threshold(&mut self, _threshold: u64) {
        todo!()
    }

    pub fn on_buffered_amount_low<F>(&mut self, _f: F) {
        todo!()
    }

    pub fn set_reliability_params(
        &mut self,
        _unordered: bool,
        _reliability_type: ReliabilityType,
        _reliability_parameter: u32,
    ) -> Result<(), Error> {
        todo!()
    }

    pub fn set_default_payload_type(&mut self, _payload_type: PayloadType) {
        todo!()
    }
}
