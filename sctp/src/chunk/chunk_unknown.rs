use std::any::Any;
use std::fmt::{Debug, Display, Formatter};

use bytes::{Bytes, BytesMut};

use crate::chunk::chunk_header::{ChunkHeader, CHUNK_HEADER_SIZE};
use crate::chunk::Chunk;

#[derive(Clone, Debug)]
pub struct ChunkUnknown {
    hdr: ChunkHeader,
    value: Bytes,
}

impl Display for ChunkUnknown {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "ChunkUnknown( {} {:?} )", self.hdr, self.value)
    }
}

impl Chunk for ChunkUnknown {
    fn header(&self) -> ChunkHeader {
        self.hdr.clone()
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }

    fn check(&self) -> crate::error::Result<()> {
        Ok(())
    }

    fn value_length(&self) -> usize {
        self.value.len()
    }

    fn marshal_to(&self, buf: &mut BytesMut) -> crate::error::Result<usize> {
        self.header().marshal_to(buf)?;
        buf.extend(&self.value);
        Ok(buf.len())
    }

    fn unmarshal(raw: &Bytes) -> crate::error::Result<Self>
    where
        Self: Sized,
    {
        let header = ChunkHeader::unmarshal(raw)?;
        let len = header.value_length();
        Ok(Self {
            hdr: header,
            value: raw.slice(CHUNK_HEADER_SIZE..CHUNK_HEADER_SIZE + len),
        })
    }
}
