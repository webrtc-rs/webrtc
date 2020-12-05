#[cfg(test)]
mod transport_layer_cc_test;

use std::fmt;
use std::io::{Cursor, Read, Write};

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use util::Error;

use super::header::*;
use crate::errors::*;
use crate::util::*;

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
enum TypeTCC {
    RunLengthChunk = 0,
    StatusVectorChunk = 1,
}

// len of packet status chunk
const PACKET_STATUS_CHUNK_LENGTH: usize = 2;

// type of packet status symbol and recv delta

// https://tools.ietf.org/html/draft-holmer-rmcat-transport-wide-cc-extensions-01#section-3.1.1
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

// for status vector chunk

// https://tools.ietf.org/html/draft-holmer-rmcat-transport-wide-cc-extensions-01#section-3.1.4
const TYPE_TCC_SYMBOL_SIZE_ONE_BIT: u16 = 0;
const TYPE_TCC_SYMBOL_SIZE_TWO_BIT: u16 = 1;

// Notice: RFC is wrong: "packet received" (0) and "packet not received" (1)
// if S == TYPE_TCCSYMBOL_SIZE_ONE_BIT, symbol list will be: TypeTCCPacketNotReceived TypeTCCPacketReceivedSmallDelta
// if S == TYPE_TCCSYMBOL_SIZE_TWO_BIT, symbol list will be same as above:

static NUM_OF_BITS_OF_SYMBOL_SIZE: [u16; 2] = [1, 2];

// PacketStatusChunk has two kinds:
// RunLengthChunk and StatusVectorChunk
#[derive(Debug, Clone, PartialEq)]
enum PacketStatusChunk {
    RunLengthChunk(RunLengthChunk),
    StatusVectorChunk(StatusVectorChunk),
}

// RunLengthChunk T=TypeTCCRunLengthChunk
// 0                   1
// 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
// |T| S |       Run Length        |
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
#[derive(Debug, Clone, PartialEq)]
struct RunLengthChunk {
    //PacketStatusChunk

    // T = TypeTCCRunLengthChunk
    type_tcc: TypeTCC,

    // S: type of packet status
    // kind: TypeTCCPacketNotReceived or...
    packet_status_symbol: TypeTCCPacket,

    // run_length: count of S
    run_length: u16,
}

impl RunLengthChunk {
    // Marshal ..
    pub fn marshal<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        // append 1 bit '0'
        let mut dst = set_nbits_of_uint16(0, 1, 0, 0)?;

        // append 2 bit packet_status_symbol
        dst = set_nbits_of_uint16(dst, 2, 1, self.packet_status_symbol as u16)?;

        // append 13 bit run_length
        dst = set_nbits_of_uint16(dst, 13, 3, self.run_length)?;

        writer.write_u16::<BigEndian>(dst)?;

        Ok(writer.flush()?)
    }

    // Unmarshal ..
    pub fn unmarshal<R: Read>(reader: &mut R) -> Result<Self, Error> {
        let b0 = reader.read_u8()?;
        let b1 = reader.read_u8()?;

        // record type
        let type_tcc = TypeTCC::RunLengthChunk;

        // get packet_status_symbol
        let packet_status_symbol = get_nbits_from_byte(b0, 1, 2);

        // get run_length
        let run_length = (get_nbits_from_byte(b0, 3, 5) << 8) + b1 as u16;

        Ok(RunLengthChunk {
            type_tcc,
            packet_status_symbol: packet_status_symbol.into(),
            run_length,
        })
    }
}

// StatusVectorChunk T=typeStatusVecotrChunk
// 0                   1
// 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
// |T|S|       symbol list         |
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
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

impl StatusVectorChunk {
    // Marshal ..
    pub fn marshal<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        // set first bit '1'
        let mut dst = set_nbits_of_uint16(0, 1, 0, 1)?;

        // set second bit symbol_size
        dst = set_nbits_of_uint16(dst, 1, 1, self.symbol_size)?;

        let num_of_bits = NUM_OF_BITS_OF_SYMBOL_SIZE[self.symbol_size as usize];
        // append 14 bit symbol_list
        for i in 0..self.symbol_list.len() {
            let index = num_of_bits * (i as u16) + 2;
            dst = set_nbits_of_uint16(dst, num_of_bits, index, self.symbol_list[i] as u16)?;
        }

        writer.write_u16::<BigEndian>(dst)?;

        Ok(writer.flush()?)
    }

    // Unmarshal ..
    pub fn unmarshal<R: Read>(reader: &mut R) -> Result<Self, Error> {
        let b0 = reader.read_u8()?;
        let b1 = reader.read_u8()?;

        let type_tcc = TypeTCC::StatusVectorChunk;
        let mut symbol_size = get_nbits_from_byte(b0, 1, 1);
        let mut symbol_list: Vec<TypeTCCPacket> = vec![];
        if symbol_size == TYPE_TCC_SYMBOL_SIZE_ONE_BIT {
            for i in 0..6 {
                symbol_list.push(get_nbits_from_byte(b0, 2 + i, 1).into());
            }
            for i in 0..8 {
                symbol_list.push(get_nbits_from_byte(b1, i, 1).into());
            }
        } else if symbol_size == TYPE_TCC_SYMBOL_SIZE_TWO_BIT {
            for i in 0..3 {
                symbol_list.push(get_nbits_from_byte(b0, 2 + i * 2, 2).into());
            }
            for i in 0..4 {
                symbol_list.push(get_nbits_from_byte(b1, i * 2, 2).into());
            }
        } else {
            symbol_size = (get_nbits_from_byte(b0, 2, 6) << 8) + (b1 as u16);
        }

        Ok(StatusVectorChunk {
            type_tcc,
            symbol_size,
            symbol_list,
        })
    }
}

//TYPE_TCC_DELTA_SCALE_FACTOR https://tools.ietf.org/html/draft-holmer-rmcat-transport-wide-cc-extensions-01#section-3.1.5
const TYPE_TCC_DELTA_SCALE_FACTOR: i64 = 250;

// RecvDelta are represented as multiples of 250us
// small delta is 1 byte: [0，63.75]ms = [0, 63750]us = [0, 255]*250us
// big delta is 2 bytes: [-8192.0, 8191.75]ms = [-8192000, 8191750]us = [-32768, 32767]*250us
// https://tools.ietf.org/html/draft-holmer-rmcat-transport-wide-cc-extensions-01#section-3.1.5
#[derive(Debug, Clone, PartialEq)]
struct RecvDelta {
    type_tcc_packet: TypeTCCPacket,
    // us
    delta: i64,
}

impl RecvDelta {
    // Marshal ..
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

// the offset after header
const BASE_SEQUENCE_NUMBER_OFFSET: usize = 8;
const PACKET_STATUS_COUNT_OFFSET: usize = 10;
const REFERENCE_TIME_OFFSET: usize = 12;
const FB_PKT_COUNT_OFFSET: usize = 15;
const PACKET_CHUNK_OFFSET: usize = 16;

// TransportLayerCC for sender-BWE
// https://tools.ietf.org/html/draft-holmer-rmcat-transport-wide-cc-extensions-01#page-5
#[derive(Debug, Clone, PartialEq)]
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
    packet_chunks: Vec<PacketStatusChunk>,

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
        for chunk in &self.packet_chunks {
            out += format!("{:?} ", chunk).as_str();
        }
        out += "\n\trecv_deltas ";
        for delta in &self.recv_deltas {
            out += format!("{:?} ", delta).as_str();
        }
        out += "\n";

        write!(f, "{}", out)
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

    fn size(&self) -> usize {
        let n = self.packet_len();
        n + get_padding(n)
    }

    // destination_ssrc returns an array of SSRC values that this packet refers to.
    pub fn destination_ssrc(&self) -> Vec<u32> {
        vec![self.media_ssrc]
    }

    // Marshal encodes the TransportLayerCC in binary
    pub fn marshal<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        self.header.marshal(writer)?;

        writer.write_u32::<BigEndian>(self.sender_ssrc)?;
        writer.write_u32::<BigEndian>(self.media_ssrc)?;
        writer.write_u16::<BigEndian>(self.base_sequence_number)?;
        writer.write_u16::<BigEndian>(self.packet_status_count)?;
        let mut reference_time_and_fb_pkt_count =
            append_nbits_to_uint32(0, 24, self.reference_time);
        reference_time_and_fb_pkt_count =
            append_nbits_to_uint32(reference_time_and_fb_pkt_count, 8, self.fb_pkt_count as u32);
        writer.write_u32::<BigEndian>(reference_time_and_fb_pkt_count)?;

        for chunk in &self.packet_chunks {
            match chunk {
                PacketStatusChunk::RunLengthChunk(chunk) => chunk.marshal(writer)?,
                PacketStatusChunk::StatusVectorChunk(chunk) => chunk.marshal(writer)?,
            };
        }

        for delta in &self.recv_deltas {
            delta.marshal(writer)?;
        }

        if self.header.padding && self.size() > self.packet_len() {
            for i in 0..self.size() - self.packet_len() {
                if i == self.size() - self.packet_len() - 1 {
                    writer.write_u8((self.size() - self.packet_len()) as u8)?;
                } else {
                    writer.write_u8(0)?;
                }
            }
        }

        Ok(writer.flush()?)
    }

    // Unmarshal ..
    pub fn unmarshal<R: Read>(reader: &mut R) -> Result<Self, Error> {
        let header = Header::unmarshal(reader)?;

        // https://tools.ietf.org/html/rfc4585#page-33
        // header's length + payload's length
        let total_length = 4 * (header.length + 1) as usize;

        if total_length <= HEADER_LENGTH + PACKET_CHUNK_OFFSET {
            return Err(ERR_PACKET_TOO_SHORT.clone());
        }

        if header.packet_type != PacketType::TransportSpecificFeedback || header.count != FORMAT_TCC
        {
            return Err(ERR_WRONG_TYPE.clone());
        }

        let sender_ssrc = reader.read_u32::<BigEndian>()?;
        let media_ssrc = reader.read_u32::<BigEndian>()?;
        let base_sequence_number = reader.read_u16::<BigEndian>()?;
        let packet_status_count = reader.read_u16::<BigEndian>()?;
        let mut buf = vec![0u8; 3];
        buf[0] = reader.read_u8()?;
        buf[1] = reader.read_u8()?;
        buf[2] = reader.read_u8()?;
        let reference_time = get_24bits_from_bytes(&buf);
        let fb_pkt_count = reader.read_u8()?;
        let mut packet_chunks = vec![];
        let mut recv_deltas = vec![];

        let mut processed_packet_num = 0i16;
        while processed_packet_num < packet_status_count as i16 {
            let b0 = reader.read_u8()?;
            let type_tcc = get_nbits_from_byte(b0, 0, 1);
            let packet_status = if type_tcc == TypeTCC::RunLengthChunk as u16 {
                let b1 = reader.read_u8()?;
                let data = vec![b0, b1];
                let mut chunk_reader = Cursor::new(&data);
                let packet_status = RunLengthChunk::unmarshal(&mut chunk_reader)?;

                let packet_number_to_process = std::cmp::min(
                    packet_status_count as i16 - processed_packet_num,
                    packet_status.run_length as i16,
                );
                if packet_status.packet_status_symbol == TypeTCCPacket::ReceivedSmallDelta
                    || packet_status.packet_status_symbol == TypeTCCPacket::ReceivedLargeDelta
                {
                    for _ in 0..packet_number_to_process {
                        recv_deltas.push(RecvDelta {
                            type_tcc_packet: if packet_status.packet_status_symbol
                                == TypeTCCPacket::ReceivedSmallDelta
                            {
                                TypeTCCPacket::ReceivedSmallDelta
                            } else {
                                TypeTCCPacket::ReceivedLargeDelta
                            },
                            delta: 0,
                        })
                    }
                }
                processed_packet_num += packet_number_to_process;
                PacketStatusChunk::RunLengthChunk(packet_status)
            } else {
                //if type_tcc == TypeTCC::StatusVectorChunk as u16 {
                let b1 = reader.read_u8()?;
                let data = vec![b0, b1];
                let mut chunk_reader = Cursor::new(&data);
                let packet_status = StatusVectorChunk::unmarshal(&mut chunk_reader)?;

                if packet_status.symbol_size == TYPE_TCC_SYMBOL_SIZE_ONE_BIT {
                    for j in 0..packet_status.symbol_list.len() {
                        if packet_status.symbol_list[j] == TypeTCCPacket::ReceivedSmallDelta {
                            recv_deltas.push(RecvDelta {
                                type_tcc_packet: TypeTCCPacket::ReceivedSmallDelta,
                                delta: 0,
                            });
                        }
                    }
                }
                if packet_status.symbol_size == TYPE_TCC_SYMBOL_SIZE_TWO_BIT {
                    for j in 0..packet_status.symbol_list.len() {
                        if packet_status.symbol_list[j] == TypeTCCPacket::ReceivedSmallDelta
                            || packet_status.symbol_list[j] == TypeTCCPacket::ReceivedLargeDelta
                        {
                            recv_deltas.push(RecvDelta {
                                type_tcc_packet: packet_status.symbol_list[j],
                                delta: 0,
                            })
                        }
                    }
                }
                processed_packet_num += packet_status.symbol_list.len() as i16;
                PacketStatusChunk::StatusVectorChunk(packet_status)
            };

            packet_chunks.push(packet_status);
        }

        for delta in &mut recv_deltas {
            if delta.type_tcc_packet == TypeTCCPacket::ReceivedSmallDelta {
                let b0 = reader.read_u8()?;
                let buf = vec![b0];
                let mut delta_reader = Cursor::new(&buf);
                *delta = RecvDelta::unmarshal(&mut delta_reader)?;
            } else {
                //TypeTCCPacketReceivedLargeDelta
                let b0 = reader.read_u8()?;
                let b1 = reader.read_u8()?;
                let buf = vec![b0, b1];
                let mut delta_reader = Cursor::new(&buf);
                *delta = RecvDelta::unmarshal(&mut delta_reader)?;
            }
        }

        Ok(TransportLayerCC {
            header,

            // SSRC of sender
            sender_ssrc,

            // SSRC of the media source
            media_ssrc,

            // Transport wide sequence of rtp extension
            base_sequence_number,

            // packet_status_count
            packet_status_count,

            // reference_time
            reference_time,

            // fb_pkt_count
            fb_pkt_count,

            // packet_chunks
            packet_chunks,

            // recv_deltas
            recv_deltas,
        })
    }
}
