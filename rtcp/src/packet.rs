use std::io::{BufReader, Read, Write};

use utils::Error;

use super::errors::*;
use super::goodbye::*;
use super::header::*;
use super::raw_packet::*;
use super::receiver_report::*;
use super::sender_report::*;
use super::source_description::*;

#[cfg(test)]
mod packet_test;

// Packet represents an RTCP packet, a protocol used for out-of-band statistics and control information for an RTP session
pub trait Packet<W: Write> {
    // DestinationSSRC returns an array of SSRC values that this packet refers to.
    fn destination_ssrc(&self) -> Vec<u32>;
    fn marshal(&self, writer: &mut W) -> Result<(), Error>;
}

//Marshal takes an array of Packets and serializes them to a single buffer
pub fn marshal<W: Write>(packets: &[impl Packet<W>], writer: &mut W) -> Result<(), Error> {
    for p in packets {
        p.marshal(writer)?;
    }
    Ok(())
}

// Unmarshal takes an entire udp datagram (which may consist of multiple RTCP packets) and
// returns the unmarshaled packets it contains.
//
// If this is a reduced-size RTCP packet a feedback packet (Goodbye, SliceLossIndication, etc)
// will be returned. Otherwise, the underlying type of the returned packet will be
// CompoundPacket.
pub fn unmarshal<W: Write>(mut raw_data: &[u8]) -> Result<Vec<Box<dyn Packet<W>>>, Error> {
    let mut packets = vec![];
    while raw_data.len() != 0 {
        if raw_data.len() < HEADER_LENGTH {
            return Err(ErrPacketTooShort.clone());
        }
        let mut header_reader = BufReader::new(&raw_data[0..HEADER_LENGTH]);
        let header = Header::unmarshal(&mut header_reader)?;

        let bytes_processed = (header.length + 1) as usize * 4;
        if bytes_processed > raw_data.len() {
            return Err(ErrPacketTooShort.clone());
        }
        let mut reader = BufReader::new(&raw_data[0..bytes_processed]);
        let packet = unmarshaler(&mut reader, header.packet_type)?;
        packets.push(packet);
        raw_data = &raw_data[bytes_processed..];
    }

    match packets.len() {
        // Empty packet
        0 => Err(ErrInvalidHeader.clone()),
        // Multiple Packets
        _ => Ok(packets),
    }
}

// unmarshaler is a factory which pulls the first RTCP packet from a bytestream,
// and returns it's parsed representation, and the amount of data that was processed.
fn unmarshaler<R: Read, W: Write>(
    reader: &mut R,
    packet_type: PacketType,
) -> Result<Box<dyn Packet<W>>, Error> {
    match packet_type {
        /*
            case TypeTransportSpecificFeedback:
                switch h.Count {
                case FormatTLN:
                    packet = new(TransportLayerNack)
                case FormatRRR:
                    packet = new(RapidResynchronizationRequest)
                default:
                    packet = new(RawPacket)
                }

            case TypePayloadSpecificFeedback:
                switch h.Count {
                case FormatPLI:
                    packet = new(PictureLossIndication)
                case FormatSLI:
                    packet = new(SliceLossIndication)
                case FormatREMB:
                    packet = new(ReceiverEstimatedMaximumBitrate)
                default:
                    packet = new(RawPacket)
                }
        */
        PacketType::TypeSenderReport => Ok(Box::new(SenderReport::unmarshal(reader)?)),
        PacketType::TypeReceiverReport => Ok(Box::new(ReceiverReport::unmarshal(reader)?)),
        PacketType::TypeSourceDescription => Ok(Box::new(SourceDescription::unmarshal(reader)?)),
        PacketType::TypeGoodbye => Ok(Box::new(Goodbye::unmarshal(reader)?)),
        _ => Ok(Box::new(RawPacket::unmarshal(reader)?)),
    }
}
