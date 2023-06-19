use std::fmt;

use bytes::{Bytes, BytesMut};

use super::chunk_header::*;
use super::chunk_type::*;
use super::*;
use crate::error_cause::*;

///Operation Error (ERROR) (9)
///
///An endpoint sends this chunk to its peer endpoint to notify it of
///certain error conditions.  It contains one or more error causes.  An
///Operation Error is not considered fatal in and of itself, but may be
///used with an ERROR chunk to report a fatal condition.  It has the
///following parameters:
/// 0                   1                   2                   3
/// 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|   Type = 9    | Chunk  Flags  |           Length              |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|                                                               |
///|                    one or more Error Causes                   |
///|                                                               |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///Chunk Flags: 8 bits
///  Set to 0 on transmit and ignored on receipt.
///Length: 16 bits (unsigned integer)
///  Set to the size of the chunk in bytes, including the chunk header
///  and all the Error Cause fields present.
#[derive(Default, Debug, Clone)]
pub(crate) struct ChunkError {
    pub(crate) error_causes: Vec<ErrorCause>,
}

/// makes ChunkError printable
impl fmt::Display for ChunkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut res = vec![self.header().to_string()];

        for cause in &self.error_causes {
            res.push(format!(" - {cause}"));
        }

        write!(f, "{}", res.join("\n"))
    }
}

impl Chunk for ChunkError {
    fn header(&self) -> ChunkHeader {
        ChunkHeader {
            typ: CT_ERROR,
            flags: 0,
            value_length: self.value_length() as u16,
        }
    }

    fn unmarshal(raw: &Bytes) -> Result<Self> {
        let header = ChunkHeader::unmarshal(raw)?;

        if header.typ != CT_ERROR {
            return Err(Error::ErrChunkTypeNotCtError);
        }

        let mut error_causes = vec![];
        let mut offset = CHUNK_HEADER_SIZE;
        while offset + 4 <= raw.len() {
            let e = ErrorCause::unmarshal(
                &raw.slice(offset..CHUNK_HEADER_SIZE + header.value_length()),
            )?;
            offset += e.length();
            error_causes.push(e);
        }

        Ok(ChunkError { error_causes })
    }

    fn marshal_to(&self, buf: &mut BytesMut) -> Result<usize> {
        self.header().marshal_to(buf)?;
        for ec in &self.error_causes {
            buf.extend(ec.marshal());
        }
        Ok(buf.len())
    }

    fn check(&self) -> Result<()> {
        Ok(())
    }

    fn value_length(&self) -> usize {
        self.error_causes
            .iter()
            .fold(0, |length, ec| length + ec.length())
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }
}
