use std::fmt;

use bytes::{Buf, BufMut, Bytes, BytesMut};

use super::chunk_header::*;
use super::chunk_type::*;
use super::*;

///This chunk shall be used by the data sender to inform the data
///receiver to adjust its cumulative received TSN point forward because
///some missing TSNs are associated with data chunks that SHOULD NOT be
///transmitted or retransmitted by the sender.
/// 0                   1                   2                   3
/// 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|   Type = 192  |  Flags = 0x00 |        Length = Variable      |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|                      New Cumulative TSN                       |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|         Stream-1              |       Stream Sequence-1       |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|                                                               |
///|                                                               |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|         Stream-N              |       Stream Sequence-N       |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
#[derive(Default, Debug, Clone)]
pub(crate) struct ChunkForwardTsn {
    /// This indicates the new cumulative TSN to the data receiver.  Upon
    /// the reception of this value, the data receiver MUST consider
    /// any missing TSNs earlier than or equal to this value as received,
    /// and stop reporting them as gaps in any subsequent SACKs.
    pub(crate) new_cumulative_tsn: u32,
    pub(crate) streams: Vec<ChunkForwardTsnStream>,
}

pub(crate) const NEW_CUMULATIVE_TSN_LENGTH: usize = 4;
pub(crate) const FORWARD_TSN_STREAM_LENGTH: usize = 4;

/// makes ChunkForwardTsn printable
impl fmt::Display for ChunkForwardTsn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut res = vec![self.header().to_string()];
        res.push(format!("New Cumulative TSN: {}", self.new_cumulative_tsn));
        for s in &self.streams {
            res.push(format!(" - si={}, ssn={}", s.identifier, s.sequence));
        }

        write!(f, "{}", res.join("\n"))
    }
}

impl Chunk for ChunkForwardTsn {
    fn header(&self) -> ChunkHeader {
        ChunkHeader {
            typ: CT_FORWARD_TSN,
            flags: 0,
            value_length: self.value_length() as u16,
        }
    }

    fn unmarshal(buf: &Bytes) -> Result<Self> {
        let header = ChunkHeader::unmarshal(buf)?;

        if header.typ != CT_FORWARD_TSN {
            return Err(Error::ErrChunkTypeNotForwardTsn);
        }

        let mut offset = CHUNK_HEADER_SIZE + NEW_CUMULATIVE_TSN_LENGTH;
        if buf.len() < offset {
            return Err(Error::ErrChunkTooShort);
        }

        let reader = &mut buf.slice(CHUNK_HEADER_SIZE..CHUNK_HEADER_SIZE + header.value_length());
        let new_cumulative_tsn = reader.get_u32();

        let mut streams = vec![];
        let mut remaining = buf.len() - offset;
        while remaining > 0 {
            let s = ChunkForwardTsnStream::unmarshal(
                &buf.slice(offset..CHUNK_HEADER_SIZE + header.value_length()),
            )?;
            offset += s.value_length();
            remaining -= s.value_length();
            streams.push(s);
        }

        Ok(ChunkForwardTsn {
            new_cumulative_tsn,
            streams,
        })
    }

    fn marshal_to(&self, writer: &mut BytesMut) -> Result<usize> {
        self.header().marshal_to(writer)?;

        writer.put_u32(self.new_cumulative_tsn);

        for s in &self.streams {
            writer.extend(s.marshal()?);
        }

        Ok(writer.len())
    }

    fn check(&self) -> Result<()> {
        Ok(())
    }

    fn value_length(&self) -> usize {
        NEW_CUMULATIVE_TSN_LENGTH + FORWARD_TSN_STREAM_LENGTH * self.streams.len()
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ChunkForwardTsnStream {
    /// This field holds a stream number that was skipped by this
    /// FWD-TSN.
    pub(crate) identifier: u16,

    /// This field holds the sequence number associated with the stream
    /// that was skipped.  The stream sequence field holds the largest
    /// stream sequence number in this stream being skipped.  The receiver
    /// of the FWD-TSN's can use the Stream-N and Stream Sequence-N fields
    /// to enable delivery of any stranded TSN's that remain on the stream
    /// re-ordering queues.  This field MUST NOT report TSN's corresponding
    /// to DATA chunks that are marked as unordered.  For ordered DATA
    /// chunks this field MUST be filled in.
    pub(crate) sequence: u16,
}

/// makes ChunkForwardTsnStream printable
impl fmt::Display for ChunkForwardTsnStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}, {}", self.identifier, self.sequence)
    }
}

impl Chunk for ChunkForwardTsnStream {
    fn header(&self) -> ChunkHeader {
        ChunkHeader {
            typ: ChunkType(0),
            flags: 0,
            value_length: self.value_length() as u16,
        }
    }

    fn unmarshal(buf: &Bytes) -> Result<Self> {
        if buf.len() < FORWARD_TSN_STREAM_LENGTH {
            return Err(Error::ErrChunkTooShort);
        }

        let reader = &mut buf.clone();
        let identifier = reader.get_u16();
        let sequence = reader.get_u16();

        Ok(ChunkForwardTsnStream {
            identifier,
            sequence,
        })
    }

    fn marshal_to(&self, writer: &mut BytesMut) -> Result<usize> {
        writer.put_u16(self.identifier);
        writer.put_u16(self.sequence);
        Ok(writer.len())
    }

    fn check(&self) -> Result<()> {
        Ok(())
    }

    fn value_length(&self) -> usize {
        FORWARD_TSN_STREAM_LENGTH
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }
}
