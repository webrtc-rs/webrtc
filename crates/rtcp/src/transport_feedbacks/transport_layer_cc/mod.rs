#[cfg(test)]
mod transport_layer_cc_test;

use crate::{error::Error, header::*, packet::*, util::*};
use util::marshal::{Marshal, MarshalSize, Unmarshal};

use anyhow::Result;
use bytes::{Buf, BufMut};
use std::any::Any;
use std::fmt;

/// https://tools.ietf.org/html/draft-holmer-rmcat-transport-wide-cc-extensions-01#page-5
/// 0                   1                   2                   3
/// 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |V=2|P|  FMT=15 |    PT=205     |           length              |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |                     SSRC of packet sender                     |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |                      SSRC of media source                     |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |      base sequence number     |      packet status count      |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |                 reference time                | fb pkt. count |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |          packet chunk         |         packet chunk          |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// .                                                               .
/// .                                                               .
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |         packet chunk          |  recv delta   |  recv delta   |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// .                                                               .
/// .                                                               .
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |           recv delta          |  recv delta   | zero padding  |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+

// for packet status chunk
/// type of packet status chunk
#[derive(PartialEq, Debug, Clone)]
#[repr(u16)]
pub enum StatusChunkTypeTcc {
    RunLengthChunk = 0,
    StatusVectorChunk = 1,
}

/// type of packet status symbol and recv delta
#[derive(PartialEq, Debug, Copy, Clone)]
#[repr(u16)]
pub enum SymbolTypeTcc {
    /// https://tools.ietf.org/html/draft-holmer-rmcat-transport-wide-cc-extensions-01#section-3.1.1
    PacketNotReceived = 0,
    /// https://tools.ietf.org/html/draft-holmer-rmcat-transport-wide-cc-extensions-01#section-3.1.1
    PacketReceivedSmallDelta = 1,
    /// https://tools.ietf.org/html/draft-holmer-rmcat-transport-wide-cc-extensions-01#section-3.1.1
    PacketReceivedLargeDelta = 2,
    /// https://tools.ietf.org/html/draft-holmer-rmcat-transport-wide-cc-extensions-01#page-7
    /// see Example 2: "packet received, w/o recv delta"
    PacketReceivedWithoutDelta = 3,
}

/// for status vector chunk
#[derive(PartialEq, Debug, Copy, Clone)]
#[repr(u16)]
pub enum SymbolSizeTypeTcc {
    /// https://tools.ietf.org/html/draft-holmer-rmcat-transport-wide-cc-extensions-01#section-3.1.4
    OneBit = 0,
    TwoBit = 1,
}

impl From<u16> for SymbolSizeTypeTcc {
    fn from(val: u16) -> Self {
        match val {
            0 => SymbolSizeTypeTcc::OneBit,
            _ => SymbolSizeTypeTcc::TwoBit,
        }
    }
}

impl Default for SymbolSizeTypeTcc {
    fn default() -> Self {
        SymbolSizeTypeTcc::OneBit
    }
}

impl From<u16> for StatusChunkTypeTcc {
    fn from(val: u16) -> Self {
        match val {
            0 => StatusChunkTypeTcc::RunLengthChunk,
            _ => StatusChunkTypeTcc::StatusVectorChunk,
        }
    }
}

impl Default for StatusChunkTypeTcc {
    fn default() -> Self {
        StatusChunkTypeTcc::RunLengthChunk
    }
}

impl From<u16> for SymbolTypeTcc {
    fn from(val: u16) -> Self {
        match val {
            0 => SymbolTypeTcc::PacketNotReceived,
            1 => SymbolTypeTcc::PacketReceivedSmallDelta,
            2 => SymbolTypeTcc::PacketReceivedLargeDelta,
            _ => SymbolTypeTcc::PacketReceivedWithoutDelta,
        }
    }
}

impl Default for SymbolTypeTcc {
    fn default() -> Self {
        SymbolTypeTcc::PacketNotReceived
    }
}

/// PacketStatusChunk has two kinds:
/// RunLengthChunk and StatusVectorChunk
pub trait PacketStatusChunk {
    fn marshal(&self) -> Result<Bytes>;
    fn unmarshal(raw_packet: &Bytes) -> Result<Self>
    where
        Self: Sized;

    fn equal(&self, other: &dyn PacketStatusChunk) -> bool;
    fn cloned(&self) -> Box<dyn PacketStatusChunk>;
    fn as_any(&self) -> &dyn Any;
}

impl PartialEq for dyn PacketStatusChunk {
    fn eq(&self, other: &dyn PacketStatusChunk) -> bool {
        self.equal(other)
    }
}

impl Clone for Box<dyn PacketStatusChunk> {
    fn clone(&self) -> Box<dyn PacketStatusChunk> {
        self.cloned()
    }
}

/// RunLengthChunk T=TypeTCCRunLengthChunk
/// 0                   1
/// 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |T| S |       Run Length        |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RunLengthChunk {
    /// T = TypeTCCRunLengthChunk
    pub type_tcc: StatusChunkTypeTcc,
    /// S: type of packet status
    /// kind: TypeTCCPacketNotReceived or...
    pub packet_status_symbol: SymbolTypeTcc,
    /// run_length: count of S
    pub run_length: u16,
}

impl PacketStatusChunk for RunLengthChunk {
    /// Marshal ..
    fn marshal(&self) -> Result<Bytes> {
        // append 1 bit '0'
        let mut dst = set_nbits_of_uint16(0, 1, 0, 0)?;

        // append 2 bit packet_status_symbol
        dst = set_nbits_of_uint16(dst, 2, 1, self.packet_status_symbol as u16)?;

        // append 13 bit run_length
        dst = set_nbits_of_uint16(dst, 13, 3, self.run_length)?;

        Ok(Bytes::from(dst.to_be_bytes().to_vec()))
    }

    /// Unmarshal ..
    fn unmarshal(raw_packet: &Bytes) -> Result<Self> {
        if raw_packet.len() != PACKET_STATUS_CHUNK_LENGTH as usize {
            return Err(Error::PacketStatusChunkLength.into());
        }

        // record type
        let type_tcc = StatusChunkTypeTcc::RunLengthChunk;

        let reader = &mut raw_packet.clone();
        let b0 = reader.get_u8();
        let b1 = reader.get_u8();

        // get PacketStatusSymbol
        let packet_status_symbol = (get_nbits_from_byte(b0, 1, 2) as u16).into();

        // get RunLength
        let run_length = ((get_nbits_from_byte(b0, 3, 5) as usize) << 8) as u16 + (b1 as u16);

        Ok(RunLengthChunk {
            type_tcc,
            packet_status_symbol,
            run_length,
        })
    }

    fn equal(&self, other: &dyn PacketStatusChunk) -> bool {
        other
            .as_any()
            .downcast_ref::<RunLengthChunk>()
            .map_or(false, |a| self == a)
    }

    fn cloned(&self) -> Box<dyn PacketStatusChunk> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// StatusVectorChunk T=typeStatusVecotrChunk
/// 0                   1
/// 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |T|S|       symbol list         |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StatusVectorChunk {
    /// T = TypeTCCRunLengthChunk
    pub type_tcc: StatusChunkTypeTcc,

    /// TypeTCCSymbolSizeOneBit or TypeTCCSymbolSizeTwoBit
    pub symbol_size: SymbolSizeTypeTcc,

    /// when symbol_size = TypeTCCSymbolSizeOneBit, symbol_list is 14*1bit:
    /// TypeTCCSymbolListPacketReceived or TypeTCCSymbolListPacketNotReceived
    /// when symbol_size = TypeTCCSymbolSizeTwoBit, symbol_list is 7*2bit:
    /// TypeTCCPacketNotReceived TypeTCCPacketReceivedSmallDelta TypeTCCPacketReceivedLargeDelta or typePacketReserved
    pub symbol_list: Vec<SymbolTypeTcc>,
}

impl PacketStatusChunk for StatusVectorChunk {
    /// Marshal ..
    fn marshal(&self) -> Result<Bytes> {
        // set first bit '1'
        let mut dst = set_nbits_of_uint16(0, 1, 0, 1)?;

        // set second bit symbol_size
        dst = set_nbits_of_uint16(dst, 1, 1, self.symbol_size as u16)?;

        let num_of_bits = NUM_OF_BITS_OF_SYMBOL_SIZE[self.symbol_size as usize];
        // append 14 bit symbol_list
        for (i, s) in self.symbol_list.iter().enumerate() {
            let index = num_of_bits * (i as u16) + 2;
            dst = set_nbits_of_uint16(dst, num_of_bits, index, *s as u16)?;
        }

        Ok(Bytes::from(dst.to_be_bytes().to_vec()))
    }

    /// Unmarshal ..
    fn unmarshal(raw_packet: &Bytes) -> Result<Self> {
        if raw_packet.len() != PACKET_STATUS_CHUNK_LENGTH {
            return Err(Error::PacketBeforeCname.into());
        }

        let type_tcc = StatusChunkTypeTcc::StatusVectorChunk;

        let reader = &mut raw_packet.clone();
        let b0 = reader.get_u8();
        let b1 = reader.get_u8();

        let symbol_size = get_nbits_from_byte(b0, 1, 1).into();

        let mut symbol_list: Vec<SymbolTypeTcc> = vec![];
        match symbol_size {
            SymbolSizeTypeTcc::OneBit => {
                for i in 0..6u16 {
                    symbol_list.push(get_nbits_from_byte(b0, 2 + i, 1).into());
                }

                for i in 0..8u16 {
                    symbol_list.push(get_nbits_from_byte(b1, i, 1).into())
                }
            }

            SymbolSizeTypeTcc::TwoBit => {
                for i in 0..3u16 {
                    symbol_list.push(get_nbits_from_byte(raw_packet[0], 2 + i * 2, 2).into());
                }

                for i in 0..4u16 {
                    symbol_list.push(get_nbits_from_byte(raw_packet[1], i * 2, 2).into());
                }
            }
        }

        Ok(StatusVectorChunk {
            type_tcc,
            symbol_size,
            symbol_list,
        })
    }

    fn equal(&self, other: &dyn PacketStatusChunk) -> bool {
        other
            .as_any()
            .downcast_ref::<StatusVectorChunk>()
            .map_or(false, |a| self == a)
    }

    fn cloned(&self) -> Box<dyn PacketStatusChunk> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// RecvDelta are represented as multiples of 250us
/// small delta is 1 byte: [0，63.75]ms = [0, 63750]us = [0, 255]*250us
/// big delta is 2 bytes: [-8192.0, 8191.75]ms = [-8192000, 8191750]us = [-32768, 32767]*250us
/// https://tools.ietf.org/html/draft-holmer-rmcat-transport-wide-cc-extensions-01#section-3.1.5
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RecvDelta {
    pub type_tcc_packet: SymbolTypeTcc,
    /// us
    pub delta: i64,
}

impl RecvDelta {
    /// Marshal ..
    pub fn marshal(&self) -> Result<Bytes> {
        let delta = self.delta / TYPE_TCC_DELTA_SCALE_FACTOR;

        // small delta
        if self.type_tcc_packet == SymbolTypeTcc::PacketReceivedSmallDelta
            && delta >= 0
            && delta <= std::u8::MAX as i64
        {
            return Ok(Bytes::from((delta as u8).to_be_bytes().to_vec()));
        }

        // big delta
        if self.type_tcc_packet == SymbolTypeTcc::PacketReceivedLargeDelta
            && delta >= std::i16::MIN as i64
            && delta <= std::u16::MAX as i64
        {
            return Ok(Bytes::from((delta as u16).to_be_bytes().to_vec()));
        }

        // overflow
        Err(Error::DeltaExceedLimit.into())
    }

    /// Unmarshal ..
    pub fn unmarshal(raw_packet: &Bytes) -> Result<Self> {
        let chunk_len = raw_packet.len();

        // must be 1 or 2 bytes
        if chunk_len != 1 && chunk_len != 2 {
            return Err(Error::DeltaExceedLimit.into());
        }

        let reader = &mut raw_packet.clone();

        let (type_tcc_packet, delta) = if chunk_len == 1 {
            (
                SymbolTypeTcc::PacketReceivedSmallDelta,
                TYPE_TCC_DELTA_SCALE_FACTOR * reader.get_u8() as i64,
            )
        } else {
            (
                SymbolTypeTcc::PacketReceivedLargeDelta,
                TYPE_TCC_DELTA_SCALE_FACTOR * reader.get_i16() as i64,
            )
        };

        Ok(RecvDelta {
            type_tcc_packet,
            delta,
        })
    }
}

/// The offset after header
const BASE_SEQUENCE_NUMBER_OFFSET: usize = 8;
/// The offset after header
const PACKET_STATUS_COUNT_OFFSET: usize = 10;
/// The offset after header
const REFERENCE_TIME_OFFSET: usize = 12;
/// The offset after header
const FB_PKT_COUNT_OFFSET: usize = 15;
/// The offset after header
const PACKET_CHUNK_OFFSET: usize = 16;
/// len of packet status chunk
const TYPE_TCC_STATUS_VECTOR_CHUNK: usize = 1;

/// https://tools.ietf.org/html/draft-holmer-rmcat-transport-wide-cc-extensions-01#section-3.1.5
const TYPE_TCC_DELTA_SCALE_FACTOR: i64 = 250;

// Notice: RFC is wrong: "packet received" (0) and "packet not received" (1)
// if S == TYPE_TCCSYMBOL_SIZE_ONE_BIT, symbol list will be: TypeTCCPacketNotReceived TypeTCCPacketReceivedSmallDelta
// if S == TYPE_TCCSYMBOL_SIZE_TWO_BIT, symbol list will be same as above:

static NUM_OF_BITS_OF_SYMBOL_SIZE: [u16; 2] = [1, 2];

/// len of packet status chunk
const PACKET_STATUS_CHUNK_LENGTH: usize = 2;

/// TransportLayerCC for sender-BWE
/// https://tools.ietf.org/html/draft-holmer-rmcat-transport-wide-cc-extensions-01#page-5
#[derive(Default, PartialEq, Clone)]
pub struct TransportLayerCc {
    /// SSRC of sender
    pub sender_ssrc: u32,
    /// SSRC of the media source
    pub media_ssrc: u32,
    /// Transport wide sequence of rtp extension
    pub base_sequence_number: u16,
    /// packet_status_count
    pub packet_status_count: u16,
    /// reference_time
    pub reference_time: u32,
    /// fb_pkt_count
    pub fb_pkt_count: u8,
    /// packet_chunks
    pub packet_chunks: Vec<Box<dyn PacketStatusChunk>>,
    /// recv_deltas
    pub recv_deltas: Vec<RecvDelta>,
}

impl fmt::Display for TransportLayerCc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = String::new();
        out += format!("TransportLayerCC:\n\tSender Ssrc {}\n", self.sender_ssrc).as_str();
        out += format!("\tMedia Ssrc {}\n", self.media_ssrc).as_str();
        out += format!("\tBase Sequence Number {}\n", self.base_sequence_number).as_str();
        out += format!("\tStatus Count {}\n", self.packet_status_count).as_str();
        out += format!("\tReference Time {}\n", self.reference_time).as_str();
        out += format!("\tFeedback Packet Count {}\n", self.fb_pkt_count).as_str();
        out += "\tpacket_chunks ";
        out += "\n\trecv_deltas ";
        for delta in &self.recv_deltas {
            out += format!("{:?} ", delta).as_str();
        }
        out += "\n";

        write!(f, "{}", out)
    }
}

impl Packet for TransportLayerCc {
    /// destination_ssrc returns an array of SSRC values that this packet refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        vec![self.media_ssrc]
    }

    fn size(&self) -> usize {
        let mut n = HEADER_LENGTH + PACKET_CHUNK_OFFSET + self.packet_chunks.len() * 2;
        for d in &self.recv_deltas {
            let delta = d.delta / TYPE_TCC_DELTA_SCALE_FACTOR;

            // small delta
            if d.type_tcc_packet == SymbolTypeTcc::PacketReceivedSmallDelta
                && delta >= 0
                && delta <= u8::MAX as i64
            {
                n += 1;
            }

            if d.type_tcc_packet == SymbolTypeTcc::PacketReceivedLargeDelta
                && delta >= i16::MIN as i64
                && delta <= i16::MAX as i64
            {
                n += 2
            }
        }
        n
    }

    fn marshal(&self) -> Result<Bytes> {
        let mut writer = BytesMut::with_capacity(self.marshal_size());

        let h = self.header();
        let data = h.marshal()?;
        writer.extend(data);

        writer.put_u32(self.sender_ssrc);
        writer.put_u32(self.media_ssrc);
        writer.put_u16(self.base_sequence_number);
        writer.put_u16(self.packet_status_count);

        let reference_time_and_fb_pkt_count = append_nbits_to_uint32(0, 24, self.reference_time);
        let reference_time_and_fb_pkt_count =
            append_nbits_to_uint32(reference_time_and_fb_pkt_count, 8, self.fb_pkt_count as u32);

        writer.put_u32(reference_time_and_fb_pkt_count);

        for chunk in &self.packet_chunks {
            let data = chunk.marshal()?;
            writer.extend(data);
        }

        for delta in &self.recv_deltas {
            let data = delta.marshal()?;
            writer.extend(data);
        }

        if self.marshal_size() > self.size() {
            while writer.len() % 4 != 0 {
                if writer.len() == self.marshal_size() - 1 {
                    writer.put_u8((self.marshal_size() - self.size()) as u8);
                } else {
                    writer.put_u8(0);
                }
            }
        }
        //FIXME: why not using put_padding(&mut writer); like others?
        Ok(writer.freeze())
    }

    /// Unmarshal ..
    fn unmarshal(raw_packet: &Bytes) -> Result<Self> {
        if raw_packet.len() < (HEADER_LENGTH + SSRC_LENGTH) {
            return Err(Error::PacketTooShort.into());
        }

        let h = Header::unmarshal(raw_packet)?;

        // https://tools.ietf.org/html/rfc4585#page-33
        // header's length + payload's length
        let total_length = 4 * (h.length + 1) as usize;

        if total_length <= HEADER_LENGTH + PACKET_CHUNK_OFFSET {
            return Err(Error::PacketTooShort.into());
        }

        if raw_packet.len() < total_length {
            return Err(Error::PacketTooShort.into());
        }

        if h.packet_type != PacketType::TransportSpecificFeedback || h.count != FORMAT_TCC {
            return Err(Error::WrongType.into());
        }

        let reader = &mut raw_packet.slice(HEADER_LENGTH..);

        let sender_ssrc = reader.get_u32();
        let media_ssrc = reader.get_u32();
        let base_sequence_number = reader.get_u16();
        let packet_status_count = reader.get_u16();

        let mut buf = vec![0u8; 3];
        buf[0] = reader.get_u8();
        buf[1] = reader.get_u8();
        buf[2] = reader.get_u8();
        let reference_time = get_24bits_from_bytes(&buf);
        let fb_pkt_count = reader.get_u8();
        let mut packet_chunks = vec![];
        let mut recv_deltas = vec![];

        let mut packet_status_pos = HEADER_LENGTH + PACKET_CHUNK_OFFSET;
        let mut processed_packet_num = 0u16;
        while processed_packet_num < packet_status_count {
            if packet_status_pos + PACKET_STATUS_CHUNK_LENGTH >= total_length {
                return Err(Error::PacketTooShort.into());
            }

            let chunk_reader =
                raw_packet.slice(packet_status_pos..packet_status_pos + PACKET_STATUS_CHUNK_LENGTH);
            let b0 = reader.get_u8();
            reader.advance(1);

            let typ = get_nbits_from_byte(b0, 0, 1);
            let initial_packet_status: Box<dyn PacketStatusChunk>;
            match typ.into() {
                StatusChunkTypeTcc::RunLengthChunk => {
                    let packet_status = RunLengthChunk::unmarshal(&chunk_reader)?;

                    let packet_number_to_process =
                        (packet_status_count - processed_packet_num).min(packet_status.run_length);

                    if packet_status.packet_status_symbol == SymbolTypeTcc::PacketReceivedSmallDelta
                        || packet_status.packet_status_symbol
                            == SymbolTypeTcc::PacketReceivedLargeDelta
                    {
                        let mut j = 0u16;

                        while j < packet_number_to_process {
                            recv_deltas.push(RecvDelta {
                                type_tcc_packet: packet_status.packet_status_symbol,
                                ..Default::default()
                            });

                            j += 1;
                        }
                    }

                    initial_packet_status = Box::new(packet_status);
                    processed_packet_num += packet_number_to_process;
                }

                StatusChunkTypeTcc::StatusVectorChunk => {
                    let packet_status = StatusVectorChunk::unmarshal(&chunk_reader)?;

                    match packet_status.symbol_size {
                        SymbolSizeTypeTcc::OneBit => {
                            for sym in &packet_status.symbol_list {
                                if *sym == SymbolTypeTcc::PacketReceivedSmallDelta {
                                    recv_deltas.push(RecvDelta {
                                        type_tcc_packet: SymbolTypeTcc::PacketReceivedSmallDelta,
                                        ..Default::default()
                                    })
                                }
                            }
                        }

                        SymbolSizeTypeTcc::TwoBit => {
                            for sym in &packet_status.symbol_list {
                                if *sym == SymbolTypeTcc::PacketReceivedSmallDelta
                                    || *sym == SymbolTypeTcc::PacketReceivedLargeDelta
                                {
                                    recv_deltas.push(RecvDelta {
                                        type_tcc_packet: *sym,
                                        ..Default::default()
                                    })
                                }
                            }
                        }
                    }

                    processed_packet_num += packet_status.symbol_list.len() as u16;
                    initial_packet_status = Box::new(packet_status);
                }
            }

            packet_status_pos += PACKET_STATUS_CHUNK_LENGTH;
            packet_chunks.push(initial_packet_status);
        }

        let mut recv_deltas_pos = packet_status_pos;

        for delta in &mut recv_deltas {
            if recv_deltas_pos >= total_length {
                return Err(Error::PacketTooShort.into());
            }

            if delta.type_tcc_packet == SymbolTypeTcc::PacketReceivedSmallDelta {
                let delta_reader = raw_packet.slice(recv_deltas_pos..recv_deltas_pos + 1);
                *delta = RecvDelta::unmarshal(&delta_reader)?;
                recv_deltas_pos += 1;
            }

            if delta.type_tcc_packet == SymbolTypeTcc::PacketReceivedLargeDelta {
                let delta_reader = raw_packet.slice(recv_deltas_pos..recv_deltas_pos + 2);
                *delta = RecvDelta::unmarshal(&delta_reader)?;
                recv_deltas_pos += 2;
            }
        }

        Ok(TransportLayerCc {
            sender_ssrc,
            media_ssrc,
            base_sequence_number,
            packet_status_count,
            reference_time,
            fb_pkt_count,
            packet_chunks,
            recv_deltas,
        })
    }

    fn equal(&self, other: &dyn Packet) -> bool {
        other
            .as_any()
            .downcast_ref::<TransportLayerCc>()
            .map_or(false, |a| self == a)
    }

    fn cloned(&self) -> Box<dyn Packet> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl TransportLayerCc {
    pub fn header(&self) -> Header {
        Header {
            padding: get_padding(self.size()) != 0,
            count: FORMAT_TCC,
            packet_type: PacketType::TransportSpecificFeedback,
            length: ((self.marshal_size() / 4) - 1) as u16,
        }
    }
}
