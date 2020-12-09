#[cfg(test)]
mod transport_layer_cc_test;

use std::fmt;
use std::io::{Cursor, Read, Write};

use byteorder::{BigEndian, ByteOrder, ReadBytesExt, WriteBytesExt};

use bytes::BytesMut;
use util::Error;

use crate::packet::Packet;

use super::header::*;
use crate::errors::*;
use crate::util as utility;

// https://tools.ietf.org/html/draft-holmer-rmcat-transport-wide-cc-extensions-01#page-5
// 0                   1                   2                   3
// 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
// |V=2|P|  FMT=15 |    PT=205     |           length              |
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
// |                     SSRC of packet sender                     |
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
// |                      SSRC of media source                     |
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
// |      base sequence number     |      packet status count      |
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
// |                 reference time                | fb pkt. count |
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
// |          packet chunk         |         packet chunk          |
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
// .                                                               .
// .                                                               .
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
// |         packet chunk          |  recv delta   |  recv delta   |
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
// .                                                               .
// .                                                               .
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
// |           recv delta          |  recv delta   | zero padding  |
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+

// for packet status chunk

// type of packet status chunk
#[derive(PartialEq, Debug, Clone)]
pub enum TypeTCC {
    RunLengthChunk = 0,
    StatusVectorChunk = 1,

    PacketStatusChunkLength = 2,
}

/// https://tools.ietf.org/html/draft-holmer-rmcat-transport-wide-cc-extensions-01#section-3.1.1
#[derive(PartialEq, Debug, Clone, Copy)]
enum TypeTCCPacket {
    NotReceived = 0,
    ReceivedSmallDelta = 1,
    ReceivedLargeDelta = 2,
    // https://tools.ietf.org/html/draft-holmer-rmcat-transport-wide-cc-extensions-01#page-7
    // see Example 2: "packet received, w/o recv delta"
    ReceivedWithoutDelta = 3,
}

impl From<u16> for TypeTCCPacket {
    fn from(val: u16) -> Self {
        use self::TypeTCCPacket::*;
        match val {
            0 => NotReceived,
            1 => ReceivedSmallDelta,
            2 => ReceivedLargeDelta,
            _ => ReceivedWithoutDelta,
        }
    }
}

/// PacketStatusChunk has two kinds:
/// RunLengthChunk and StatusVectorChunk
pub trait PacketStatusChunk {
    fn marshal(&self) -> Result<BytesMut, Error>;
    fn unmarshal(&self, rawPacket: &mut BytesMut) -> Result<(), Error>;
}

/// RunLengthChunk T=TypeTCCRunLengthChunk
/// 0                   1
/// 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |T| S |       Run Length        |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
#[derive(Debug, Clone, PartialEq)]
struct RunLengthChunk {
    // T = TypeTCCRunLengthChunk
    type_tcc: TypeTCC,

    // S: type of packet status
    // kind: TypeTCCPacketNotReceived or...
    packet_status_symbol: TypeTCCPacket,

    // run_length: count of S
    run_length: u16,
}

impl PacketStatusChunk for RunLengthChunk {
    // Marshal ..
    fn marshal(&self) -> Result<BytesMut, Error> {
        let chunks = vec![0u8; 2];

        // append 1 bit '0'
        let mut dst = utility::set_nbits_of_uint16(0, 1, 0, 0)?;

        // append 2 bit packet_status_symbol
        dst = utility::set_nbits_of_uint16(dst, 2, 1, self.packet_status_symbol as u16)?;

        // append 13 bit run_length
        dst = utility::set_nbits_of_uint16(dst, 13, 3, self.run_length)?;

        BigEndian::write_u16(&mut chunks, dst);

        Ok(chunks[..].into())
    }

    // Unmarshal ..
    fn unmarshal(&self, raw_packet: &mut BytesMut) -> Result<(), Error> {
        if raw_packet.len() != PACKET_STATUS_CHUNK_LENGTH as usize {
            return Err(Error::new(
                "packet status chunk must be 2 bytes".to_string(),
            ));
        }

        // record type
        self.type_tcc = TypeTCC::RunLengthChunk;

        // get PacketStatusSymbol
        // r.PacketStatusSymbol = uint16(rawPacket[0] >> 5 & 0x03)
        self.packet_status_symbol = utility::get_nbits_from_byte(raw_packet[0], 1, 2).into();

        // get RunLength
        // r.RunLength = uint16(rawPacket[0]&0x1F)*256 + uint16(rawPacket[1])
        self.run_length =
            utility::get_nbits_from_byte(raw_packet[0], 3, 5) << 8 + raw_packet[1] as u16;

        Ok(())
    }
}

/// StatusVectorChunk T=typeStatusVecotrChunk
/// 0                   1
/// 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |T|S|       symbol list         |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
#[derive(Debug, Clone, PartialEq)]
struct StatusVectorChunk {
    // T = TypeTCCRunLengthChunk
    type_tcc: TypeTCC,

    // TypeTCCSymbolSizeOneBit or TypeTCCSymbolSizeTwoBit
    symbol_size: u16,

    // when symbol_size = TypeTCCSymbolSizeOneBit, symbol_list is 14*1bit:
    // TypeTCCSymbolListPacketReceived or TypeTCCSymbolListPacketNotReceived
    // when symbol_size = TypeTCCSymbolSizeTwoBit, symbol_list is 7*2bit:
    // TypeTCCPacketNotReceived TypeTCCPacketReceivedSmallDelta TypeTCCPacketReceivedLargeDelta or typePacketReserved
    symbol_list: Vec<TypeTCCPacket>,
}

impl PacketStatusChunk for StatusVectorChunk {
    // Marshal ..
    fn marshal(&self) -> Result<BytesMut, Error> {
        let chunk = vec![0u8; 2];

        // set first bit '1'
        let mut dst = utility::set_nbits_of_uint16(0, 1, 0, 1)?;

        // set second bit symbol_size
        dst = utility::set_nbits_of_uint16(dst, 1, 1, self.symbol_size)?;

        let num_of_bits = NUM_OF_BITS_OF_SYMBOL_SIZE[self.symbol_size as usize];
        // append 14 bit symbol_list
        for i in 0..self.symbol_list.len() {
            let index = num_of_bits * (i as u16) + 2;
            dst =
                utility::set_nbits_of_uint16(dst, num_of_bits, index, self.symbol_list[i] as u16)?;
        }

        BigEndian::write_u16(&mut chunk, dst);
        // set SymbolList(bit8-15)
        // chunk[1] = uint8(r.SymbolList) & 0x0f
        Ok(chunk[..].into())
    }

    // Unmarshal ..
    fn unmarshal(&self, raw_packet: &mut BytesMut) -> Result<(), Error> {
        todo!()
    }
}

/// RecvDelta are represented as multiples of 250us
/// small delta is 1 byte: [0，63.75]ms = [0, 63750]us = [0, 255]*250us
/// big delta is 2 bytes: [-8192.0, 8191.75]ms = [-8192000, 8191750]us = [-32768, 32767]*250us
/// https://tools.ietf.org/html/draft-holmer-rmcat-transport-wide-cc-extensions-01#section-3.1.5
#[derive(Debug, Clone, PartialEq)]
struct RecvDelta {
    type_tcc_packet: TypeTCCPacket,
    // us
    delta: i64,
}

impl RecvDelta {
    /// Marshal ..
    pub fn marshal<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        let delta = self.delta / TYPE_TCC_DELTA_SCALE_FACTOR;

        //small delta
        if self.type_tcc_packet == TypeTCCPacket::ReceivedSmallDelta
            && delta >= 0
            && delta <= u8::MAX as i64
        {
            writer.write_u8(delta as u8)?;
            return Ok(writer.flush()?);
        }

        //big delta
        if self.type_tcc_packet == TypeTCCPacket::ReceivedLargeDelta
            && delta >= i16::MIN as i64
            && delta <= i16::MAX as i64
        {
            writer.write_i16::<BigEndian>(delta as i16)?;
            return Ok(writer.flush()?);
        }

        //overflow
        Err(ERR_DELTA_EXCEED_LIMIT.clone())
    }

    // Unmarshal ..
    pub fn unmarshal<R: Read>(reader: &mut R) -> Result<Self, Error> {
        let mut buf = vec![];
        reader.read_to_end(&mut buf)?;

        let chunk_len = buf.len();

        // must be 1 or 2 bytes
        if chunk_len != 1 && chunk_len != 2 {
            return Err(ERR_DELTA_EXCEED_LIMIT.clone());
        }

        let (type_tcc_packet, delta) = if chunk_len == 1 {
            (
                TypeTCCPacket::ReceivedSmallDelta,
                TYPE_TCC_DELTA_SCALE_FACTOR * (buf[0] as i64),
            )
        } else {
            let delta = ((buf[0] as u16) << 8 | buf[1] as u16) as i16;

            (
                TypeTCCPacket::ReceivedLargeDelta,
                TYPE_TCC_DELTA_SCALE_FACTOR * (delta as i64),
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

/// https://tools.ietf.org/html/draft-holmer-rmcat-transport-wide-cc-extensions-01#section-3.1.5
const TYPE_TCC_DELTA_SCALE_FACTOR: i64 = 250;

// for status vector chunk

/// https://tools.ietf.org/html/draft-holmer-rmcat-transport-wide-cc-extensions-01#section-3.1.4
const TYPE_TCC_SYMBOL_SIZE_ONE_BIT: u16 = 0;
const TYPE_TCC_SYMBOL_SIZE_TWO_BIT: u16 = 1;

// Notice: RFC is wrong: "packet received" (0) and "packet not received" (1)
// if S == TYPE_TCCSYMBOL_SIZE_ONE_BIT, symbol list will be: TypeTCCPacketNotReceived TypeTCCPacketReceivedSmallDelta
// if S == TYPE_TCCSYMBOL_SIZE_TWO_BIT, symbol list will be same as above:

static NUM_OF_BITS_OF_SYMBOL_SIZE: [u16; 2] = [1, 2];

/// len of packet status chunk
const PACKET_STATUS_CHUNK_LENGTH: usize = 2;

/// TransportLayerCC for sender-BWE
/// https://tools.ietf.org/html/draft-holmer-rmcat-transport-wide-cc-extensions-01#page-5
#[derive(Default)]
pub struct TransportLayerCC {
    // header
    header: Header,

    // SSRC of sender
    sender_ssrc: u32,

    // SSRC of the media source
    media_ssrc: u32,

    // Transport wide sequence of rtp extension
    base_sequence_number: u16,

    // packet_status_count
    packet_status_count: u16,

    // reference_time
    reference_time: u32,

    // fb_pkt_count
    fb_pkt_count: u8,

    // packet_chunks
    packet_chunks: Vec<Box<dyn PacketStatusChunk>>,

    // recv_deltas
    recv_deltas: Vec<RecvDelta>,
}

impl fmt::Display for TransportLayerCC {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = format!("TransportLayerCC:\n\tHeader {:?}\n", self.header);
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

impl Packet for TransportLayerCC {
    fn unmarshal(&self, raw_packet: &mut BytesMut) -> Result<(), Error> {
        todo!()
    }

    fn marshal(&self) -> Result<BytesMut, Error> {
        todo!()
    }

    // destination_ssrc returns an array of SSRC values that this packet refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        vec![self.media_ssrc]
    }
}

impl TransportLayerCC {
    fn packet_len(&self) -> usize {
        let mut n = HEADER_LENGTH + PACKET_CHUNK_OFFSET + self.packet_chunks.len() * 2;
        for d in &self.recv_deltas {
            let delta = d.delta / TYPE_TCC_DELTA_SCALE_FACTOR;

            // small delta
            if d.type_tcc_packet == TypeTCCPacket::ReceivedSmallDelta
                && delta >= 0
                && delta <= u8::MAX as i64
            {
                n += 1;
            }

            if d.type_tcc_packet == TypeTCCPacket::ReceivedLargeDelta
                && delta >= i16::MIN as i64
                && delta <= i16::MAX as i64
            {
                n += 2
            }
        }
        n
    }

    fn len(&self) -> usize {
        let n = self.packet_len();
        n + utility::get_padding(n)
    }
}
