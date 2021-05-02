use super::{chunk_header::*, chunk_type::*, *};
use crate::param::{param_header::*, *};

use bytes::{Bytes, BytesMut};
use std::fmt;

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
pub(crate) struct ChunkHeartbeat {
    pub(crate) params: Vec<Box<dyn Param>>,
}

/// makes ChunkHeartbeat printable
impl fmt::Display for ChunkHeartbeat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.header())
    }
}

impl Chunk for ChunkHeartbeat {
    fn unmarshal(raw: &Bytes) -> Result<Self, Error> {
        let header = ChunkHeader::unmarshal(raw)?;

        if header.typ != ChunkType::Heartbeat {
            return Err(Error::ErrChunkTypeNotHeartbeat);
        }

        if raw.len() <= CHUNK_HEADER_SIZE {
            return Err(Error::ErrHeartbeatNotLongEnoughInfo);
        }

        let params = vec![];
        /*TODO:
        pType, err := parseParamType(raw[chunkHeaderSize:])
        if err != nil {
            return fmt.Errorf("%w: %v", errParseParamTypeFailed, err)
        }
        if pType != HEARTBEAT_INFO {
            return fmt.Errorf("%w: instead have %s", errHeartbeatParam, pType.String())
        }

        p, err := buildParam(pType, raw[chunkHeaderSize:])
        if err != nil {
            return fmt.Errorf("%w: %v", errHeartbeatChunkUnmarshal, err)
        }
        h.params = append(h.params, p)*/

        Ok(ChunkHeartbeat { params })
    }

    fn marshal_to(&self, buf: &mut BytesMut) -> Result<usize, Error> {
        self.header().marshal_to(buf)?;
        for p in &self.params {
            buf.extend(p.marshal()?);
        }
        Ok(buf.len())
    }

    fn check(&self) -> Result<bool, Error> {
        Ok(false)
    }

    fn value_length(&self) -> usize {
        self.params.iter().fold(0, |length, p| {
            length + PARAM_HEADER_LENGTH + p.value_length()
        })
    }
}

impl ChunkHeartbeat {
    pub(crate) fn header(&self) -> ChunkHeader {
        ChunkHeader {
            typ: ChunkType::Heartbeat,
            flags: 0,
            value_length: self.value_length() as u16,
        }
    }
}
