use crate::{
    compound_packet::*, error::Error, goodbye::*, header::*,
    payload_feedbacks::full_intra_request::*, payload_feedbacks::picture_loss_indication::*,
    payload_feedbacks::receiver_estimated_maximum_bitrate::*,
    payload_feedbacks::slice_loss_indication::*, raw_packet::*, receiver_report::*,
    sender_report::*, source_description::*,
    transport_feedbacks::rapid_resynchronization_request::*,
    transport_feedbacks::transport_layer_cc::*, transport_feedbacks::transport_layer_nack::*,
    util::*,
};

use bytes::Bytes;
use std::any::Any;

/// Packet represents an RTCP packet, a protocol used for out-of-band statistics and control information for an RTP session
pub trait Packet {
    /// DestinationSSRC returns an array of SSRC values that this packet refers to.
    fn destination_ssrc(&self) -> Vec<u32>;
    fn size(&self) -> usize;
    fn marshal(&self) -> Result<Bytes, Error>;
    fn unmarshal(raw_packet: &Bytes) -> Result<Self, Error>
    where
        Self: Sized;

    fn equal_to(&self, other: &dyn Packet) -> bool;
    fn clone_to(&self) -> Box<dyn Packet>;
    fn as_any(&self) -> &dyn Any;

    fn marshal_size(&self) -> usize {
        let l = self.size();
        // align to 32-bit boundary
        l + get_padding(l)
    }
}

impl PartialEq for dyn Packet {
    fn eq(&self, other: &Self) -> bool {
        self.equal_to(other)
    }
}

impl Clone for Box<dyn Packet> {
    fn clone(&self) -> Box<dyn Packet> {
        self.clone_to()
    }
}

/// Unmarshal takes an entire udp datagram (which may consist of multiple RTCP packets) and
/// returns the unmarshaled packets it contains.
///
/// If this is a reduced-size RTCP packet a feedback packet (Goodbye, SliceLossIndication, etc)
/// will be returned. Otherwise, the underlying type of the returned packet will be
/// CompoundPacket.
pub fn unmarshal(raw_data: &Bytes) -> Result<Box<dyn Packet>, Error> {
    let mut packets = vec![];

    let mut raw_data = raw_data.clone();
    while !raw_data.is_empty() {
        let (p, processed) = unmarshaller(&raw_data)?;
        packets.push(p);
        raw_data = raw_data.split_off(processed);
    }

    match packets.len() {
        // Empty Packet
        0 => Err(Error::InvalidHeader),

        // Single Packet
        1 => packets.pop().ok_or(Error::BadFirstPacket),

        // Compound Packet
        _ => Ok(Box::new(CompoundPacket(packets))),
    }
}

/// unmarshaller is a factory which pulls the first RTCP packet from a bytestream,
/// and returns it's parsed representation, and the amount of data that was processed.
pub(crate) fn unmarshaller(raw_data: &Bytes) -> Result<(Box<dyn Packet>, usize), Error> {
    let h = Header::unmarshal(&raw_data)?;

    let bytes_processed = (h.length as usize + 1) * 4;
    if bytes_processed > raw_data.len() {
        return Err(Error::PacketTooShort);
    }

    let in_packet = raw_data.slice(..bytes_processed);

    let p: Box<dyn Packet> = match h.packet_type {
        PacketType::SenderReport => Box::new(SenderReport::unmarshal(&in_packet)?),
        PacketType::ReceiverReport => Box::new(ReceiverReport::unmarshal(&in_packet)?),
        PacketType::SourceDescription => Box::new(SourceDescription::unmarshal(&in_packet)?),
        PacketType::Goodbye => Box::new(Goodbye::unmarshal(&in_packet)?),

        PacketType::TransportSpecificFeedback => match h.count {
            FORMAT_TLN => Box::new(TransportLayerNack::unmarshal(&in_packet)?),
            FORMAT_RRR => Box::new(RapidResynchronizationRequest::unmarshal(&in_packet)?),
            FORMAT_TCC => Box::new(TransportLayerCc::unmarshal(&in_packet)?),
            _ => Box::new(RawPacket::unmarshal(&in_packet)?),
        },

        PacketType::PayloadSpecificFeedback => match h.count {
            FORMAT_PLI => Box::new(PictureLossIndication::unmarshal(&in_packet)?),
            FORMAT_SLI => Box::new(SliceLossIndication::unmarshal(&in_packet)?),
            FORMAT_REMB => Box::new(ReceiverEstimatedMaximumBitrate::unmarshal(&in_packet)?),
            FORMAT_FIR => Box::new(FullIntraRequest::unmarshal(&in_packet)?),
            _ => Box::new(RawPacket::unmarshal(&in_packet)?),
        },
        _ => Box::new(RawPacket::unmarshal(&in_packet)?),
    };

    Ok((p, bytes_processed))
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::reception_report::*;

    #[test]
    fn test_packet_unmarshal() {
        let data = Bytes::from_static(&[
            // Receiver Report (offset=0)
            0x81, 0xc9, 0x0, 0x7, // v=2, p=0, count=1, RR, len=7
            0x90, 0x2f, 0x9e, 0x2e, // ssrc=0x902f9e2e
            0xbc, 0x5e, 0x9a, 0x40, // ssrc=0xbc5e9a40
            0x0, 0x0, 0x0, 0x0, // fracLost=0, totalLost=0
            0x0, 0x0, 0x46, 0xe1, // lastSeq=0x46e1
            0x0, 0x0, 0x1, 0x11, // jitter=273
            0x9, 0xf3, 0x64, 0x32, // lsr=0x9f36432
            0x0, 0x2, 0x4a, 0x79, // delay=150137
            // Source Description (offset=32)
            0x81, 0xca, 0x0, 0xc, // v=2, p=0, count=1, SDES, len=12
            0x90, 0x2f, 0x9e, 0x2e, // ssrc=0x902f9e2e
            0x1, 0x26, // CNAME, len=38
            0x7b, 0x39, 0x63, 0x30, 0x30, 0x65, 0x62, 0x39, 0x32, 0x2d, 0x31, 0x61, 0x66, 0x62,
            0x2d, 0x39, 0x64, 0x34, 0x39, 0x2d, 0x61, 0x34, 0x37, 0x64, 0x2d, 0x39, 0x31, 0x66,
            0x36, 0x34, 0x65, 0x65, 0x65, 0x36, 0x39, 0x66, 0x35,
            0x7d, // text="{9c00eb92-1afb-9d49-a47d-91f64eee69f5}"
            0x0, 0x0, 0x0, 0x0, // END + padding
            // Goodbye (offset=84)
            0x81, 0xcb, 0x0, 0x1, // v=2, p=0, count=1, BYE, len=1
            0x90, 0x2f, 0x9e, 0x2e, // source=0x902f9e2e
            0x81, 0xce, 0x0, 0x2, // Picture Loss Indication (offset=92)
            0x90, 0x2f, 0x9e, 0x2e, // sender=0x902f9e2e
            0x90, 0x2f, 0x9e, 0x2e, // media=0x902f9e2e
            0x85, 0xcd, 0x0, 0x2, // RapidResynchronizationRequest (offset=104)
            0x90, 0x2f, 0x9e, 0x2e, // sender=0x902f9e2e
            0x90, 0x2f, 0x9e, 0x2e, // media=0x902f9e2e
        ]);

        let packet = unmarshal(&data).expect("Error unmarshalling packets");

        let a = ReceiverReport {
            ssrc: 0x902f9e2e,
            reports: vec![ReceptionReport {
                ssrc: 0xbc5e9a40,
                fraction_lost: 0,
                total_lost: 0,
                last_sequence_number: 0x46e1,
                jitter: 273,
                last_sender_report: 0x9f36432,
                delay: 150137,
            }],
            ..Default::default()
        };

        let b = SourceDescription {
            chunks: vec![SourceDescriptionChunk {
                source: 0x902f9e2e,
                items: vec![SourceDescriptionItem {
                    sdes_type: SdesType::SdesCname,
                    text: Bytes::from_static(b"{9c00eb92-1afb-9d49-a47d-91f64eee69f5}"),
                }],
            }],
        };

        let c = Goodbye {
            sources: vec![0x902f9e2e],
            ..Default::default()
        };

        let d = PictureLossIndication {
            sender_ssrc: 0x902f9e2e,
            media_ssrc: 0x902f9e2e,
        };

        let e = RapidResynchronizationRequest {
            sender_ssrc: 0x902f9e2e,
            media_ssrc: 0x902f9e2e,
        };

        let expected: Box<dyn Packet> = Box::new(CompoundPacket(vec![
            Box::new(a),
            Box::new(b),
            Box::new(c),
            Box::new(d),
            Box::new(e),
        ]));

        assert!(packet == expected, "Invalid packets");
    }

    #[test]
    fn test_packet_unmarshal_empty() -> Result<(), Error> {
        let result = unmarshal(&Bytes::new());
        if let Err(got) = result {
            let want = Error::InvalidHeader;
            assert_eq!(got, want, "Unmarshal(nil) err = {}, want {}", got, want);
        } else {
            assert!(false, "want error");
        }

        Ok(())
    }

    #[test]
    fn test_packet_invalid_header_length() -> Result<(), Error> {
        let data = Bytes::from_static(&[
            // Goodbye (offset=84)
            // v=2, p=0, count=1, BYE, len=100
            0x81, 0xcb, 0x0, 0x64,
        ]);

        let result = unmarshal(&data);
        if let Err(got) = result {
            let want = Error::PacketTooShort;
            assert_eq!(
                got, want,
                "Unmarshal(invalid_header_length) err = {}, want {}",
                got, want
            );
        } else {
            assert!(false, "want error");
        }

        Ok(())
    }
}
