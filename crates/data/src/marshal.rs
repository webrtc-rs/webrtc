use bytes::{Buf, BufMut, Bytes, BytesMut};

pub trait MarshalSize {
    fn marshal_size(&self) -> usize;
}

pub trait Marshal: MarshalSize {
    type Error;

    fn marshal_to<B>(&self, buf: &mut B) -> Result<usize, Self::Error>
    where
        B: BufMut;

    fn marshal(&self) -> Result<Bytes, Self::Error> {
        let mut buf = BytesMut::with_capacity(self.marshal_size());
        let _ = self.marshal_to(&mut buf)?;
        Ok(buf.freeze())
    }
}

pub trait Unmarshal: Sized + MarshalSize {
    type Error;

    fn unmarshal_from<B>(buf: &mut B) -> Result<Self, Self::Error>
    where
        B: Buf;
}
