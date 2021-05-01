use super::chunk_header::*; //, *
use crate::error_cause::*;
//use crate::chunk::chunk_type::ChunkType;

//use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::fmt;

///Abort represents an SCTP Chunk of type ABORT
///
///The ABORT chunk is sent to the peer of an association to close the
///association.  The ABORT chunk may contain Cause Parameters to inform
///the receiver about the reason of the abort.  DATA chunks MUST NOT be
///bundled with ABORT.  Control chunks (except for INIT, INIT ACK, and
///SHUTDOWN COMPLETE) MAY be bundled with an ABORT, but they MUST be
///placed before the ABORT in the SCTP packet or they will be ignored by
///the receiver.
///
/// 0                   1                   2                   3
/// 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|   Type = 6    |Reserved     |T|           Length              |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|                                                               |
///|                   zero or more Error Causes                   |
///|                                                               |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+

pub(crate) struct ChunkAbort {
    header: ChunkHeader,
    error_causes: Vec<ErrorCause>,
}

// String makes chunkAbort printable
impl fmt::Display for ChunkAbort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut res = vec![self.header.to_string()];

        for cause in &self.error_causes {
            res.push(format!(" - {}", cause.to_string()));
        }

        write!(f, "{}", res.join("\n"))
    }
}

/*
impl Chunk for ChunkAbort {
    fn unmarshal(raw: &Bytes) -> Result<Self, Error> {
        let header  = ChunkHeader::unmarshal(raw)?;

        if header.typ != ChunkType::Abort {
            return Err(Error::ErrChunkTypeNotAbort);
        }

        let mut error_causes = vec![];
        let mut offset = CHUNK_HEADER_SIZE;
        while offset + 4 <= raw.len() {
            let e = BuildErrorCause(&raw.slice(offset..))?;
            offset += e.length();
            error_causes.push(e);
        }

        Ok(ChunkAbort{
            header,
            error_causes,
        })
    }

    func (a *chunkAbort) marshal() ([]byte, error) {
        a.chunkHeader.typ = ctAbort
        a.flags = 0x00
        a.raw = []byte{}
        for _, ec := range a.error_causes {
            raw, err := ec.marshal()
            if err != nil {
                return nil, err
            }
            a.raw = append(a.raw, raw...)
        }
        return a.chunkHeader.marshal()
    }

    fn check(&self) ->Result<bool, Error> {
        Ok(false)
    }

    fn value_length(&self) -> usize {
        self.header.value_length()
    }
}*/
