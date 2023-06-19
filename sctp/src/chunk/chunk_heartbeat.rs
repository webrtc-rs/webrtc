use std::fmt;

use bytes::{Bytes, BytesMut};

use super::chunk_header::*;
use super::chunk_type::*;
use super::*;
use crate::param::param_header::*;
use crate::param::param_type::*;
use crate::param::*;

///chunkHeartbeat represents an SCTP Chunk of type HEARTBEAT
///
///An endpoint should send this chunk to its peer endpoint to probe the
///reachability of a particular destination transport address defined in
///the present association.
///
///The parameter field contains the Heartbeat Information, which is a
///variable-length opaque data structure understood only by the sender.
///
///
/// 0                   1                   2                   3
/// 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|   Type = 4    | Chunk  Flags  |      Heartbeat Length         |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|                                                               |
///|            Heartbeat Information TLV (Variable-Length)        |
///|                                                               |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///
///Defined as a variable-length parameter using the format described
///in Section 3.2.1, i.e.:
///
///Variable Parameters                  Status     Type Value
///-------------------------------------------------------------
///heartbeat Info                       Mandatory   1
#[derive(Default, Debug)]
pub(crate) struct ChunkHeartbeat {
    pub(crate) params: Vec<Box<dyn Param + Send + Sync>>,
}

/// makes ChunkHeartbeat printable
impl fmt::Display for ChunkHeartbeat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.header())
    }
}

impl Chunk for ChunkHeartbeat {
    fn header(&self) -> ChunkHeader {
        ChunkHeader {
            typ: CT_HEARTBEAT,
            flags: 0,
            value_length: self.value_length() as u16,
        }
    }

    fn unmarshal(raw: &Bytes) -> Result<Self> {
        let header = ChunkHeader::unmarshal(raw)?;

        if header.typ != CT_HEARTBEAT {
            return Err(Error::ErrChunkTypeNotHeartbeat);
        }

        if raw.len() <= CHUNK_HEADER_SIZE {
            return Err(Error::ErrHeartbeatNotLongEnoughInfo);
        }

        let p =
            build_param(&raw.slice(CHUNK_HEADER_SIZE..CHUNK_HEADER_SIZE + header.value_length()))?;
        if p.header().typ != ParamType::HeartbeatInfo {
            return Err(Error::ErrHeartbeatParam);
        }
        let params = vec![p];

        Ok(ChunkHeartbeat { params })
    }

    fn marshal_to(&self, buf: &mut BytesMut) -> Result<usize> {
        self.header().marshal_to(buf)?;
        for p in &self.params {
            buf.extend(p.marshal()?);
        }
        Ok(buf.len())
    }

    fn check(&self) -> Result<()> {
        Ok(())
    }

    fn value_length(&self) -> usize {
        self.params.iter().fold(0, |length, p| {
            length + PARAM_HEADER_LENGTH + p.value_length()
        })
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }
}
