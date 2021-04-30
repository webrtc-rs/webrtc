use super::chunk_header::*;
use crate::error_cause::*;

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
    error_causes: Vec<Box<dyn ErrorCause>>,
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
    fn unmarshal(buf: &Bytes) -> Result<Self, Error> {
        if err := a.chunkHeader.unmarshal(raw); err != nil {
            return err
        }

        if a.typ != ctAbort {
            return fmt.Errorf("%w: actually is %s", errChunkTypeNotAbort, a.typ.String())
        }

        offset := chunkHeaderSize
        for {
            if len(raw)-offset < 4 {
                break
            }

            e, err := buildErrorCause(raw[offset:])
            if err != nil {
                return fmt.Errorf("%w: %v", errBuildAbortChunkFailed, err)
            }

            offset += int(e.length())
            a.error_causes = append(a.error_causes, e)
        }
        return nil
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
