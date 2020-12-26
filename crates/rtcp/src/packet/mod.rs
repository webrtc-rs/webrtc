use bytes::BytesMut;
use errors::*;
use full_intra_request::FullIntraRequest;
use util::Error;

use crate::{errors, raw_packet};

use super::{
    full_intra_request, goodbye, header, header::Header, header::PacketType,
    picture_loss_indication, rapid_resynchronization_request, raw_packet::RawPacket,
    receiver_estimated_maximum_bitrate, receiver_report, sender_report, slice_loss_indication,
    source_description, transport_layer_cc, transport_layer_nack,
};

mod packet_test;

/// Packet represents an RTCP packet, a protocol used for out-of-band statistics and control information for an RTP session
pub trait Packet {
    fn trait_eq(&self, other: &dyn Packet) -> bool;
    fn as_any(&self) -> &dyn std::any::Any;

    fn destination_ssrc(&self) -> Vec<u32>;

    fn marshal(&self) -> Result<BytesMut, Error>;
    fn unmarshal(&mut self, raw_packet: &mut BytesMut) -> Result<(), Error>;
}

impl PartialEq for dyn Packet {
    fn eq(&self, other: &Self) -> bool {
        self.trait_eq(other)
    }
}

pub fn unmarshal(mut raw_data: BytesMut) -> Result<Vec<Box<dyn Packet>>, Error> {
    let mut packets = vec![];

    while !raw_data.is_empty() {
        let (p, processed) = unmarshaller(&mut raw_data)?;

        packets.push(p);
        raw_data = raw_data.split_off(processed);
    }

    match packets.len() {
        // Empty packet
        0 => Err(ERR_INVALID_HEADER.to_owned()),

        // Multiple packets
        _ => Ok(packets),
    }
}

/// Marshal takes an array of Packets and serializes them to a single buffer
pub fn marshal(packets: &[Box<dyn Packet>]) -> Result<BytesMut, Error> {
    let mut out = BytesMut::new();

    for packet in packets {
        let a = packet.marshal()?;

        out.extend(a);
    }

    Ok(out)
}

/// unmarshaller is a factory which pulls the first RTCP packet from a bytestream,
/// and returns it's parsed representation, and the amount of data that was processed.
pub(crate) fn unmarshaller(mut raw_data: &mut BytesMut) -> Result<(Box<dyn Packet>, usize), Error> {
    let mut h = Header::default();

    h.unmarshal(&mut raw_data)?;

    let bytes_processed = (h.length as usize + 1) * 4;
    if bytes_processed > raw_data.len() {
        return Err(ERR_PACKET_TOO_SHORT.to_owned());
    }

    let mut in_packet = raw_data[..bytes_processed].into();

    let mut packet: Box<dyn Packet> = match h.packet_type {
        PacketType::SenderReport => Box::new(sender_report::SenderReport::default()),

        PacketType::ReceiverReport => Box::new(receiver_report::ReceiverReport::default()),

        PacketType::SourceDescription => Box::new(source_description::SourceDescription::default()),
        PacketType::Goodbye => Box::new(goodbye::Goodbye::default()),

        PacketType::TransportSpecificFeedback => match h.count {
            header::FORMAT_TLN => Box::new(transport_layer_nack::TransportLayerNack::default()),

            header::FORMAT_RRR => {
                Box::new(rapid_resynchronization_request::RapidResynchronizationRequest::default())
            }

            header::FORMAT_TCC => Box::new(transport_layer_cc::TransportLayerCC::default()),

            _ => Box::new(raw_packet::RawPacket::default()),
        },

        PacketType::PayloadSpecificFeedback => match h.count {
            header::FORMAT_PLI => {
                Box::new(picture_loss_indication::PictureLossIndication::default())
            }

            header::FORMAT_SLI => Box::new(slice_loss_indication::SliceLossIndication::default()),

            header::FORMAT_REMB => Box::new(
                receiver_estimated_maximum_bitrate::ReceiverEstimatedMaximumBitrate::default(),
            ),

            header::FORMAT_FIR => Box::new(FullIntraRequest::default()),

            _ => Box::new(RawPacket::default()),
        },

        _ => Box::new(RawPacket::default()),
    };

    packet.unmarshal(&mut in_packet)?;

    Ok((packet, bytes_processed))
}
