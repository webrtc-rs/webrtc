pub mod error;
pub mod exact_size_buf;

use anyhow::Result;
use bytes::{Buf, Bytes, BytesMut};
use error::Error;

pub trait MarshalSize {
    fn marshal_size(&self) -> usize;
}

pub trait Marshal: MarshalSize {
    fn marshal_to(&self, buf: &mut [u8]) -> Result<usize>;

    fn marshal(&self) -> Result<Bytes> {
        let l = self.marshal_size();
        let mut buf = BytesMut::with_capacity(l);
        buf.resize(l, 0);
        let n = self.marshal_to(&mut buf)?;
        if n != l {
            Err(Error::new(format!("marshal_to output size {}, but expect {}", n, l)).into())
        } else {
            Ok(buf.freeze())
        }
    }
}

pub trait Unmarshal: Sized + MarshalSize {
    fn unmarshal<B>(buf: &mut B) -> Result<Self>
    where
        B: Buf;
}
