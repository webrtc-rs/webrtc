use bytes::BytesMut;
use full_intra_request::FullIntraRequest;
use util::Error;

use crate::raw_packet;

use super::{
    full_intra_request, goodbye, header, header::Header, header::PacketType,
    picture_loss_indication, rapid_resynchronization_request, raw_packet::RawPacket,
    receiver_estimated_maximum_bitrate, receiver_report, sender_report, slice_loss_indication,
    source_description, transport_layer_cc, transport_layer_nack,
};

#[cfg(test)]
mod packet_test;

/// Packet represents an RTCP packet, a protocol used for out-of-band statistics and control information for an RTP session
pub trait Packet {
    fn destination_ssrc(&self) -> Vec<u32>;

    fn marshal(&self) -> Result<BytesMut, Error>;
    fn unmarshal(&self, raw_packet: Vec<u8>) -> Result<(), Error>;
}

pub fn unmarshal(raw_data: &mut BytesMut) -> Result<Vec<impl Packet>, Error> {
    let mut packets = vec![];

    while raw_data.len() != 0 {
        let (p, processed) = unmarshaller(raw_data)?;

        packets.push(p);
        raw_data = raw_data[processed..];
    }

    match packets.len() {
        // Empty packet
        0 => Err(Error::new("packet too short".to_string())),

        // Multiple packets
        _ => Ok(packets),
    }
}

/// Marshal takes an array of Packets and serializes them to a single buffer
pub fn marshal(packets: &[impl Packet]) -> Result<BytesMut, Error> {
    let mut out = BytesMut::new();

    for packet in packets {
        let a = packet.marshal()?;

        out.extend(a);
    }

    Ok(out)
}

/// unmarshaller is a factory which pulls the first RTCP packet from a bytestream,
/// and returns it's parsed representation, and the amount of data that was processed.
fn unmarshaller(raw_data: &mut BytesMut) -> Result<(impl Packet, usize), Error> {
    let h = Header::default();

    h.unmarshal(&mut raw_data)?;

    let mut bytes_processed = (h.length as usize + 1) * 4;
    if bytes_processed > raw_data.len() {
        return Err(Error::new("packet too short".to_string()));
    }

    let mut in_packet = &raw_data[..bytes_processed];

    let packet = match h.packet_type {
        PacketType::SenderReport => sender_report::SenderReport::default(),

        PacketType::ReceiverReport => receiver_report::ReceiverReport::default(),

        PacketType::SourceDescription => source_description::SourceDescription::default(),

        PacketType::Goodbye => goodbye::Goodbye::default(),

        PacketType::TransportSpecificFeedback => match h.count {
            header::FORMAT_TLN => transport_layer_nack::TransportLayerNack::default(),

            header::FORMAT_RRR => {
                rapid_resynchronization_request::RapidResynchronizationRequest::default()
            }

            header::FORMAT_TCC => transport_layer_cc::TransportLayerCC::default(),

            _ => raw_packet::RawPacket::default(),
        },

        PacketType::PayloadSpecificFeedback => match h.count {
            header::FORMAT_PLI => picture_loss_indication::PictureLossIndication::default(),

            header::FORMAT_SLI => slice_loss_indication::SliceLossIndication::default(),

            header::FORMAT_REMB => {
                receiver_estimated_maximum_bitrate::ReceiverEstimatedMaximumBitrate::default()
            }

            header::FORMAT_FIR => FullIntraRequest::default(),

            _ => RawPacket::default(),
        },

        _ => RawPacket::default(),
    };

    packet.unmarshal(&mut in_packet)?;

    Ok(packet, bytes_processed)
}
