use std::fmt;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::SystemTime;

use bytes::{Buf, BufMut, Bytes, BytesMut};
use portable_atomic::AtomicBool;

use super::chunk_header::*;
use super::chunk_type::*;
use super::*;

pub(crate) const PAYLOAD_DATA_ENDING_FRAGMENT_BITMASK: u8 = 1;
pub(crate) const PAYLOAD_DATA_BEGINNING_FRAGMENT_BITMASK: u8 = 2;
pub(crate) const PAYLOAD_DATA_UNORDERED_BITMASK: u8 = 4;
pub(crate) const PAYLOAD_DATA_IMMEDIATE_SACK: u8 = 8;
pub(crate) const PAYLOAD_DATA_HEADER_SIZE: usize = 12;

/// PayloadProtocolIdentifier is an enum for DataChannel payload types
/// PayloadProtocolIdentifier enums
/// https://www.iana.org/assignments/sctp-parameters/sctp-parameters.xhtml#sctp-parameters-25
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
#[repr(C)]
pub enum PayloadProtocolIdentifier {
    Dcep = 50,
    String = 51,
    Binary = 53,
    StringEmpty = 56,
    BinaryEmpty = 57,
    #[default]
    Unknown,
}

impl fmt::Display for PayloadProtocolIdentifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            PayloadProtocolIdentifier::Dcep => "WebRTC DCEP",
            PayloadProtocolIdentifier::String => "WebRTC String",
            PayloadProtocolIdentifier::Binary => "WebRTC Binary",
            PayloadProtocolIdentifier::StringEmpty => "WebRTC String (Empty)",
            PayloadProtocolIdentifier::BinaryEmpty => "WebRTC Binary (Empty)",
            _ => "Unknown Payload Protocol Identifier",
        };
        write!(f, "{s}")
    }
}

impl From<u32> for PayloadProtocolIdentifier {
    fn from(v: u32) -> PayloadProtocolIdentifier {
        match v {
            50 => PayloadProtocolIdentifier::Dcep,
            51 => PayloadProtocolIdentifier::String,
            53 => PayloadProtocolIdentifier::Binary,
            56 => PayloadProtocolIdentifier::StringEmpty,
            57 => PayloadProtocolIdentifier::BinaryEmpty,
            _ => PayloadProtocolIdentifier::Unknown,
        }
    }
}

///chunkPayloadData represents an SCTP Chunk of type DATA
///
/// 0                   1                   2                   3
/// 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|   Type = 0    | Reserved|U|B|E|    Length                     |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|                              TSN                              |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|      Stream Identifier S      |   Stream Sequence Number n    |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|                  Payload Protocol Identifier                  |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|                                                               |
///|                 User Data (seq n of Stream S)                 |
///|                                                               |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///
///
///An unfragmented user message shall have both the B and E bits set to
///'1'.  Setting both B and E bits to '0' indicates a middle fragment of
///a multi-fragment user message, as summarized in the following table:
///   B E                  Description
///============================================================
///|  1 0 | First piece of a fragmented user message          |
///+----------------------------------------------------------+
///|  0 0 | Middle piece of a fragmented user message         |
///+----------------------------------------------------------+
///|  0 1 | Last piece of a fragmented user message           |
///+----------------------------------------------------------+
///|  1 1 | Unfragmented message                              |
///============================================================
///|             Table 1: Fragment Description Flags          |
///============================================================
#[derive(Debug, Clone)]
pub struct ChunkPayloadData {
    pub(crate) unordered: bool,
    pub(crate) beginning_fragment: bool,
    pub(crate) ending_fragment: bool,
    pub(crate) immediate_sack: bool,

    pub(crate) tsn: u32,
    pub(crate) stream_identifier: u16,
    pub(crate) stream_sequence_number: u16,
    pub(crate) payload_type: PayloadProtocolIdentifier,
    pub(crate) user_data: Bytes,

    /// Whether this data chunk was acknowledged (received by peer)
    pub(crate) acked: bool,
    pub(crate) miss_indicator: u32,

    /// Partial-reliability parameters used only by sender
    pub(crate) since: SystemTime,
    /// number of transmission made for this chunk
    pub(crate) nsent: u32,

    /// valid only with the first fragment
    pub(crate) abandoned: Arc<AtomicBool>,
    /// valid only with the first fragment
    pub(crate) all_inflight: Arc<AtomicBool>,

    /// Retransmission flag set when T1-RTX timeout occurred and this
    /// chunk is still in the inflight queue
    pub(crate) retransmit: bool,
}

impl Default for ChunkPayloadData {
    fn default() -> Self {
        ChunkPayloadData {
            unordered: false,
            beginning_fragment: false,
            ending_fragment: false,
            immediate_sack: false,
            tsn: 0,
            stream_identifier: 0,
            stream_sequence_number: 0,
            payload_type: PayloadProtocolIdentifier::default(),
            user_data: Bytes::new(),
            acked: false,
            miss_indicator: 0,
            since: SystemTime::now(),
            nsent: 0,
            abandoned: Arc::new(AtomicBool::new(false)),
            all_inflight: Arc::new(AtomicBool::new(false)),
            retransmit: false,
        }
    }
}

/// makes chunkPayloadData printable
impl fmt::Display for ChunkPayloadData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}\n{}", self.header(), self.tsn)
    }
}

impl Chunk for ChunkPayloadData {
    fn header(&self) -> ChunkHeader {
        let mut flags: u8 = 0;
        if self.ending_fragment {
            flags = 1;
        }
        if self.beginning_fragment {
            flags |= 1 << 1;
        }
        if self.unordered {
            flags |= 1 << 2;
        }
        if self.immediate_sack {
            flags |= 1 << 3;
        }

        ChunkHeader {
            typ: CT_PAYLOAD_DATA,
            flags,
            value_length: self.value_length() as u16,
        }
    }

    fn unmarshal(raw: &Bytes) -> Result<Self> {
        let header = ChunkHeader::unmarshal(raw)?;

        if header.typ != CT_PAYLOAD_DATA {
            return Err(Error::ErrChunkTypeNotPayloadData);
        }

        let immediate_sack = (header.flags & PAYLOAD_DATA_IMMEDIATE_SACK) != 0;
        let unordered = (header.flags & PAYLOAD_DATA_UNORDERED_BITMASK) != 0;
        let beginning_fragment = (header.flags & PAYLOAD_DATA_BEGINNING_FRAGMENT_BITMASK) != 0;
        let ending_fragment = (header.flags & PAYLOAD_DATA_ENDING_FRAGMENT_BITMASK) != 0;

        // validity of value_length is checked in ChunkHeader::unmarshal
        if header.value_length() < PAYLOAD_DATA_HEADER_SIZE {
            return Err(Error::ErrChunkPayloadSmall);
        }

        let reader = &mut raw.slice(CHUNK_HEADER_SIZE..CHUNK_HEADER_SIZE + header.value_length());

        let tsn = reader.get_u32();
        let stream_identifier = reader.get_u16();
        let stream_sequence_number = reader.get_u16();
        let payload_type: PayloadProtocolIdentifier = reader.get_u32().into();
        let user_data = raw.slice(
            CHUNK_HEADER_SIZE + PAYLOAD_DATA_HEADER_SIZE..CHUNK_HEADER_SIZE + header.value_length(),
        );

        Ok(ChunkPayloadData {
            unordered,
            beginning_fragment,
            ending_fragment,
            immediate_sack,

            tsn,
            stream_identifier,
            stream_sequence_number,
            payload_type,
            user_data,
            acked: false,
            miss_indicator: 0,
            since: SystemTime::now(),
            nsent: 0,
            abandoned: Arc::new(AtomicBool::new(false)),
            all_inflight: Arc::new(AtomicBool::new(false)),
            retransmit: false,
        })
    }

    fn marshal_to(&self, writer: &mut BytesMut) -> Result<usize> {
        self.header().marshal_to(writer)?;

        writer.put_u32(self.tsn);
        writer.put_u16(self.stream_identifier);
        writer.put_u16(self.stream_sequence_number);
        writer.put_u32(self.payload_type as u32);
        writer.extend_from_slice(&self.user_data);

        Ok(writer.len())
    }

    fn check(&self) -> Result<()> {
        Ok(())
    }

    fn value_length(&self) -> usize {
        PAYLOAD_DATA_HEADER_SIZE + self.user_data.len()
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }
}

impl ChunkPayloadData {
    pub(crate) fn abandoned(&self) -> bool {
        let (abandoned, all_inflight) = (
            self.abandoned.load(Ordering::SeqCst),
            self.all_inflight.load(Ordering::SeqCst),
        );

        abandoned && all_inflight
    }

    pub(crate) fn set_abandoned(&self, abandoned: bool) {
        self.abandoned.store(abandoned, Ordering::SeqCst);
    }

    pub(crate) fn set_all_inflight(&mut self) {
        if self.ending_fragment {
            self.all_inflight.store(true, Ordering::SeqCst);
        }
    }
}
