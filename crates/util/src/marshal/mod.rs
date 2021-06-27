pub mod exact_size_buf;

use anyhow::Result;
use bytes::{Buf, BufMut, Bytes, BytesMut};

pub trait MarshalSize {
    fn marshal_size(&self) -> usize;
}

pub trait Marshal: MarshalSize {
    fn marshal_to<B>(&self, buf: &mut B) -> Result<usize>
    where
        B: BufMut;

    fn marshal(&self) -> Result<Bytes> {
        let mut buf = BytesMut::with_capacity(self.marshal_size());
        let _ = self.marshal_to(&mut buf)?;
        Ok(buf.freeze())
    }
}

pub trait Unmarshal: Sized + MarshalSize {
    fn unmarshal<B>(buf: &mut B) -> Result<Self>
    where
        B: Buf;
}
