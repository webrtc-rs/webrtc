use super::{chunk_header::*, chunk_type::*, *};
use crate::param::*;

use bytes::{Bytes, BytesMut};
use std::fmt;

///chunkHeartbeatAck represents an SCTP Chunk of type HEARTBEAT ACK
///
///An endpoint should send this chunk to its peer endpoint as a response
///to a HEARTBEAT chunk (see Section 8.3).  A HEARTBEAT ACK is always
///sent to the source IP address of the IP datagram containing the
///HEARTBEAT chunk to which this ack is responding.
///
///The parameter field contains a variable-length opaque data structure.
///
/// 0                   1                   2                   3
/// 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|   Type = 5    | Chunk  Flags  |    Heartbeat Ack Length       |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|                                                               |
///|            Heartbeat Information TLV (Variable-Length)        |
///|                                                               |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///
///
///Defined as a variable-length parameter using the format described
///in Section 3.2.1, i.e.:
///
///Variable Parameters                  Status     Type Value
///-------------------------------------------------------------
///Heartbeat Info                       Mandatory   1
pub(crate) struct ChunkHeartbeatAck {
    params: Vec<Box<dyn Param>>,
}

/// makes ChunkHeartbeatAck printable
impl fmt::Display for ChunkHeartbeatAck {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.header())
    }
}

impl Chunk for ChunkHeartbeatAck {
    fn unmarshal(raw: &Bytes) -> Result<Self, Error> {
        let header = ChunkHeader::unmarshal(raw)?;

        if header.typ != ChunkType::Heartbeat {
            return Err(Error::ErrChunkTypeNotHeartbeat);
        }

        if raw.len() <= CHUNK_HEADER_SIZE {
            return Err(Error::ErrHeartbeatNotLongEnoughInfo);
        }
        let params = vec![];
        //TODO
        Ok(ChunkHeartbeatAck { params })
    }

    fn marshal_to(&self, buf: &mut BytesMut) -> Result<usize, Error> {
        if self.params.len() != 1 {
            return Err(Error::ErrHeartbeatAckParams);
        }

        self.header().marshal_to(buf)?;
        for p in &self.params {
            buf.extend(p.marshal()?);
        }
        Ok(buf.len())

        /*TODO:
        switch h.params[0].(type) {
        case *paramHeartbeatInfo:
            // ParamHeartbeatInfo is valid
        default:
            return nil, errHeartbeatAckNotHeartbeatInfo
        }

        out := make([]byte, 0)
        for idx, p := range h.params {
            pp, err := p.marshal()
            if err != nil {
                return nil, fmt.Errorf("%w: %v", errHeartbeatAckMarshalParam, err)
            }

            out = append(out, pp...)

            // Chunks (including Type, Length, and Value fields) are padded out
            // by the sender with all zero bytes to be a multiple of 4 bytes
            // long.  This padding MUST NOT be more than 3 bytes in total.  The
            // Chunk Length value does not include terminating padding of the
            // chunk.  *However, it does include padding of any variable-length
            // parameter except the last parameter in the chunk.*  The receiver
            // MUST ignore the padding.
            if idx != len(h.params)-1 {
                out = padByte(out, getPadding(len(pp)))
            }
        }

        h.chunkHeader.typ = ctHeartbeatAck
        h.chunkHeader.raw = out

        return h.chunkHeader.marshal()*/
    }

    fn check(&self) -> Result<bool, Error> {
        Ok(false)
    }

    fn value_length(&self) -> usize {
        self.params.iter().fold(0, |length, p| length + p.length())
    }
}

impl ChunkHeartbeatAck {
    pub(crate) fn header(&self) -> ChunkHeader {
        ChunkHeader {
            typ: ChunkType::HeartbeatAck,
            flags: 0,
            value_length: self.value_length() as u16,
        }
    }
}
