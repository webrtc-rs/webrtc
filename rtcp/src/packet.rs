use std::io::{BufReader, Read, Write};

use util::Error;

use super::compound_packet::*;
use super::errors::*;
use super::full_intra_request::*;
use super::goodbye::*;
use super::header::*;
use super::picture_loss_indication::*;
use super::rapid_resynchronization_request::*;
use super::raw_packet::*;
use super::receiver_estimated_maximum_bitrate::*;
use super::receiver_report::*;
use super::sender_report::*;
use super::slice_loss_indication::*;
use super::source_description::*;
use super::transport_layer_cc::*;
use super::transport_layer_nack::*;

#[cfg(test)]
mod packet_test;

// Packet represents an RTCP packet, a protocol used for out-of-band statistics and control information for an RTP session
#[derive(Debug, Clone)]
pub enum Packet {
    SenderReport(SenderReport),
    ReceiverReport(ReceiverReport),
    SourceDescription(SourceDescription),
    Goodbye(Goodbye),
    RawPacket(RawPacket),

    TransportLayerNack(TransportLayerNack),
    RapidResynchronizationRequest(RapidResynchronizationRequest),
    TransportLayerCC(TransportLayerCC),

    PictureLossIndication(PictureLossIndication),
    SliceLossIndication(SliceLossIndication),
    ReceiverEstimatedMaximumBitrate(ReceiverEstimatedMaximumBitrate),
    FullIntraRequest(FullIntraRequest),

    CompoundPacket(CompoundPacket),
}

impl Packet {
    pub fn marshal<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        match self {
            Packet::SenderReport(p) => p.marshal(writer)?,
            Packet::ReceiverReport(p) => p.marshal(writer)?,
            Packet::SourceDescription(p) => p.marshal(writer)?,
            Packet::Goodbye(p) => p.marshal(writer)?,
            Packet::RawPacket(p) => p.marshal(writer)?,

            Packet::TransportLayerNack(p) => p.marshal(writer)?,
            Packet::RapidResynchronizationRequest(p) => p.marshal(writer)?,
            Packet::TransportLayerCC(p) => p.marshal(writer)?,

            Packet::PictureLossIndication(p) => p.marshal(writer)?,
            Packet::SliceLossIndication(p) => p.marshal(writer)?,
            Packet::ReceiverEstimatedMaximumBitrate(p) => p.marshal(writer)?,
            Packet::FullIntraRequest(p) => p.marshal(writer)?,

            Packet::CompoundPacket(p) => p.marshal(writer)?,
        };
        Ok(())
    }

    pub fn destination_ssrc(&self) -> Vec<u32> {
        match self {
            Packet::SenderReport(p) => p.destination_ssrc(),
            Packet::ReceiverReport(p) => p.destination_ssrc(),
            Packet::SourceDescription(p) => p.destination_ssrc(),
            Packet::Goodbye(p) => p.destination_ssrc(),
            Packet::RawPacket(p) => p.destination_ssrc(),

            Packet::TransportLayerNack(p) => p.destination_ssrc(),
            Packet::RapidResynchronizationRequest(p) => p.destination_ssrc(),
            Packet::TransportLayerCC(p) => p.destination_ssrc(),

            Packet::PictureLossIndication(p) => p.destination_ssrc(),
            Packet::SliceLossIndication(p) => p.destination_ssrc(),
            Packet::ReceiverEstimatedMaximumBitrate(p) => p.destination_ssrc(),
            Packet::FullIntraRequest(p) => p.destination_ssrc(),

            Packet::CompoundPacket(p) => p.destination_ssrc(),
        }
    }
}

//Marshal takes an array of Packets and serializes them to a single buffer
pub fn marshal<W: Write>(packets: &[Packet], writer: &mut W) -> Result<(), Error> {
    for packet in packets {
        packet.marshal(writer)?;
    }
    Ok(())
}

// Unmarshal takes an entire udp datagram (which may consist of multiple RTCP packets) and
// returns the unmarshaled packets it contains.
//
// If this is a reduced-size RTCP packet a feedback packet (Goodbye, SliceLossIndication, etc)
// will be returned. Otherwise, the underlying type of the returned packet will be
// CompoundPacket.
pub fn unmarshal(mut raw_data: &[u8]) -> Result<Packet, Error> {
    let mut packets = vec![];
    while !raw_data.is_empty() {
        if raw_data.len() < HEADER_LENGTH {
            return Err(ERR_PACKET_TOO_SHORT.clone());
        }
        let mut header_reader = BufReader::new(&raw_data[0..HEADER_LENGTH]);
        let header = Header::unmarshal(&mut header_reader)?;

        let bytes_processed = (header.length + 1) as usize * 4;
        if bytes_processed > raw_data.len() {
            return Err(ERR_PACKET_TOO_SHORT.clone());
        }
        let mut reader = BufReader::new(&raw_data[0..bytes_processed]);
        let packet = unmarshaler(&mut reader, &header)?;
        packets.push(packet);
        raw_data = &raw_data[bytes_processed..];
    }

    match packets.len() {
        // Empty packet
        0 => Err(ERR_INVALID_HEADER.clone()),
        1 => packets.pop().ok_or_else(|| ERR_BAD_FIRST_PACKET.clone()),
        // Multiple Packets
        _ => Ok(Packet::CompoundPacket(CompoundPacket(packets))),
    }
}

// unmarshaler is a factory which pulls the first RTCP packet from a bytestream,
// and returns it's parsed representation, and the amount of data that was processed.
fn unmarshaler<R: Read>(reader: &mut R, header: &Header) -> Result<Packet, Error> {
    match header.packet_type {
        PacketType::SenderReport => Ok(Packet::SenderReport(SenderReport::unmarshal(reader)?)),
        PacketType::ReceiverReport => {
            Ok(Packet::ReceiverReport(ReceiverReport::unmarshal(reader)?))
        }
        PacketType::SourceDescription => Ok(Packet::SourceDescription(
            SourceDescription::unmarshal(reader)?,
        )),
        PacketType::Goodbye => Ok(Packet::Goodbye(Goodbye::unmarshal(reader)?)),
        PacketType::TransportSpecificFeedback => match header.count {
            FORMAT_TLN => Ok(Packet::TransportLayerNack(TransportLayerNack::unmarshal(
                reader,
            )?)),
            FORMAT_RRR => Ok(Packet::RapidResynchronizationRequest(
                RapidResynchronizationRequest::unmarshal(reader)?,
            )),
            FORMAT_TCC => Ok(Packet::TransportLayerCC(TransportLayerCC::unmarshal(
                reader,
            )?)),
            _ => Ok(Packet::RawPacket(RawPacket::unmarshal(reader)?)),
        },
        PacketType::PayloadSpecificFeedback => match header.count {
            FORMAT_PLI => Ok(Packet::PictureLossIndication(
                PictureLossIndication::unmarshal(reader)?,
            )),
            FORMAT_SLI => Ok(Packet::SliceLossIndication(SliceLossIndication::unmarshal(
                reader,
            )?)),
            FORMAT_REMB => Ok(Packet::ReceiverEstimatedMaximumBitrate(
                ReceiverEstimatedMaximumBitrate::unmarshal(reader)?,
            )),
            FORMAT_FIR => Ok(Packet::FullIntraRequest(FullIntraRequest::unmarshal(
                reader,
            )?)),
            _ => Ok(Packet::RawPacket(RawPacket::unmarshal(reader)?)),
        },
        _ => Ok(Packet::RawPacket(RawPacket::unmarshal(reader)?)),
    }
}
