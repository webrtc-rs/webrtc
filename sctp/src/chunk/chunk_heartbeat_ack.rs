use std::fmt;

use bytes::{Bytes, BytesMut};

use super::chunk_header::*;
use super::chunk_type::*;
use super::*;
use crate::param::param_header::*;
use crate::param::param_type::ParamType;
use crate::param::*;
use crate::util::get_padding_size;

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
#[derive(Default, Debug)]
pub(crate) struct ChunkHeartbeatAck {
    pub(crate) params: Vec<Box<dyn Param + Send + Sync>>,
}

/// makes ChunkHeartbeatAck printable
impl fmt::Display for ChunkHeartbeatAck {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.header())
    }
}

impl Chunk for ChunkHeartbeatAck {
    fn header(&self) -> ChunkHeader {
        ChunkHeader {
            typ: CT_HEARTBEAT_ACK,
            flags: 0,
            value_length: self.value_length() as u16,
        }
    }

    fn unmarshal(raw: &Bytes) -> Result<Self> {
        let header = ChunkHeader::unmarshal(raw)?;

        if header.typ != CT_HEARTBEAT_ACK {
            return Err(Error::ErrChunkTypeNotHeartbeatAck);
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

        Ok(ChunkHeartbeatAck { params })
    }

    fn marshal_to(&self, buf: &mut BytesMut) -> Result<usize> {
        if self.params.len() != 1 {
            return Err(Error::ErrHeartbeatAckParams);
        }
        if self.params[0].header().typ != ParamType::HeartbeatInfo {
            return Err(Error::ErrHeartbeatAckNotHeartbeatInfo);
        }

        self.header().marshal_to(buf)?;
        for (idx, p) in self.params.iter().enumerate() {
            let pp = p.marshal()?;
            let pp_len = pp.len();
            buf.extend(pp);

            // Chunks (including Type, Length, and Value fields) are padded out
            // by the sender with all zero bytes to be a multiple of 4 bytes
            // long.  This PADDING MUST NOT be more than 3 bytes in total.  The
            // Chunk Length value does not include terminating PADDING of the
            // chunk.  *However, it does include PADDING of any variable-length
            // parameter except the last parameter in the chunk.*  The receiver
            // MUST ignore the PADDING.
            if idx != self.params.len() - 1 {
                let cnt = get_padding_size(pp_len);
                buf.extend(vec![0u8; cnt]);
            }
        }
        Ok(buf.len())
    }

    fn check(&self) -> Result<()> {
        Ok(())
    }

    fn value_length(&self) -> usize {
        let mut l = 0;
        for (idx, p) in self.params.iter().enumerate() {
            let p_len = PARAM_HEADER_LENGTH + p.value_length();
            l += p_len;
            if idx != self.params.len() - 1 {
                l += get_padding_size(p_len);
            }
        }
        l
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }
}
