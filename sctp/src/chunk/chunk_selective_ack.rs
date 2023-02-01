use super::{chunk_header::*, chunk_type::*, *};

use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::fmt;

///chunkSelectiveAck represents an SCTP Chunk of type SACK
///
///This chunk is sent to the peer endpoint to acknowledge received DATA
///chunks and to inform the peer endpoint of gaps in the received
///subsequences of DATA chunks as represented by their TSNs.
///0                   1                   2                   3
///0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|   Type = 3    |Chunk  Flags   |      Chunk Length             |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|                      Cumulative TSN Ack                       |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|          Advertised Receiver Window Credit (a_rwnd)           |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///| Number of Gap Ack Blocks = N  |  Number of Duplicate TSNs = X |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|  Gap Ack Block #1 Start       |   Gap Ack Block #1 End        |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|                                                               |
///|                              ...                              |
///|                                                               |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|   Gap Ack Block #N Start      |  Gap Ack Block #N End         |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|                       Duplicate TSN 1                         |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|                                                               |
///|                              ...                              |
///|                                                               |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|                       Duplicate TSN X                         |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
#[derive(Debug, Default, Copy, Clone)]
pub(crate) struct GapAckBlock {
    pub(crate) start: u16,
    pub(crate) end: u16,
}

/// makes gapAckBlock printable
impl fmt::Display for GapAckBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} - {}", self.start, self.end)
    }
}

#[derive(Default, Debug)]
pub(crate) struct ChunkSelectiveAck {
    pub(crate) cumulative_tsn_ack: u32,
    pub(crate) advertised_receiver_window_credit: u32,
    pub(crate) gap_ack_blocks: Vec<GapAckBlock>,
    pub(crate) duplicate_tsn: Vec<u32>,
}

/// makes chunkSelectiveAck printable
impl fmt::Display for ChunkSelectiveAck {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut res = format!(
            "SACK cumTsnAck={} arwnd={} dupTsn={:?}",
            self.cumulative_tsn_ack, self.advertised_receiver_window_credit, self.duplicate_tsn
        );

        for gap in &self.gap_ack_blocks {
            res += format!("\n gap ack: {gap}").as_str();
        }

        write!(f, "{res}")
    }
}

pub(crate) const SELECTIVE_ACK_HEADER_SIZE: usize = 12;

impl Chunk for ChunkSelectiveAck {
    fn header(&self) -> ChunkHeader {
        ChunkHeader {
            typ: CT_SACK,
            flags: 0,
            value_length: self.value_length() as u16,
        }
    }

    fn unmarshal(raw: &Bytes) -> Result<Self> {
        let header = ChunkHeader::unmarshal(raw)?;

        if header.typ != CT_SACK {
            return Err(Error::ErrChunkTypeNotSack);
        }

        // validity of value_length is checked in ChunkHeader::unmarshal
        if header.value_length() < SELECTIVE_ACK_HEADER_SIZE {
            return Err(Error::ErrSackSizeNotLargeEnoughInfo);
        }

        let reader = &mut raw.slice(CHUNK_HEADER_SIZE..CHUNK_HEADER_SIZE + header.value_length());

        let cumulative_tsn_ack = reader.get_u32();
        let advertised_receiver_window_credit = reader.get_u32();
        let gap_ack_blocks_len = reader.get_u16() as usize;
        let duplicate_tsn_len = reader.get_u16() as usize;

        // Here we must account for case where the buffer contains another chunk
        // right after this one. Testing for equality would incorrectly fail the
        // parsing of this chunk and incorrectly close the transport.

        // validity of value_length is checked in ChunkHeader::unmarshal
        if header.value_length()
            < SELECTIVE_ACK_HEADER_SIZE + (4 * gap_ack_blocks_len + 4 * duplicate_tsn_len)
        {
            return Err(Error::ErrSackSizeNotLargeEnoughInfo);
        }

        let mut gap_ack_blocks = vec![];
        let mut duplicate_tsn = vec![];
        for _ in 0..gap_ack_blocks_len {
            let start = reader.get_u16();
            let end = reader.get_u16();
            gap_ack_blocks.push(GapAckBlock { start, end });
        }
        for _ in 0..duplicate_tsn_len {
            duplicate_tsn.push(reader.get_u32());
        }

        Ok(ChunkSelectiveAck {
            cumulative_tsn_ack,
            advertised_receiver_window_credit,
            gap_ack_blocks,
            duplicate_tsn,
        })
    }

    fn marshal_to(&self, writer: &mut BytesMut) -> Result<usize> {
        self.header().marshal_to(writer)?;

        writer.put_u32(self.cumulative_tsn_ack);
        writer.put_u32(self.advertised_receiver_window_credit);
        writer.put_u16(self.gap_ack_blocks.len() as u16);
        writer.put_u16(self.duplicate_tsn.len() as u16);
        for g in &self.gap_ack_blocks {
            writer.put_u16(g.start);
            writer.put_u16(g.end);
        }
        for t in &self.duplicate_tsn {
            writer.put_u32(*t);
        }

        Ok(writer.len())
    }

    fn check(&self) -> Result<()> {
        Ok(())
    }

    fn value_length(&self) -> usize {
        SELECTIVE_ACK_HEADER_SIZE + self.gap_ack_blocks.len() * 4 + self.duplicate_tsn.len() * 4
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }
}
