use bytes::{Bytes, BytesMut};

use crate::compound_packet::CompoundPacket;
use crate::{
    error::Error, goodbye::*, header::*, raw_packet::*, receiver_report::*, sender_report::*,
    source_description::*,
};

/* full_intra_request,
picture_loss_indication, rapid_resynchronization_request,
receiver_estimated_maximum_bitrate, , slice_loss_indication,
, transport_layer_cc, transport_layer_nack,*/

/// Packet represents an RTCP packet, a protocol used for out-of-band statistics and control information for an RTP session
pub trait Packet {
    /// DestinationSSRC returns an array of SSRC values that this packet refers to.
    fn destination_ssrc(&self) -> Vec<u32>;
    fn marshal_size(&self) -> usize;
    fn marshal(&self) -> Result<Bytes, Error>;
    fn unmarshal(raw_packet: &Bytes) -> Result<Self, Error>
    where
        Self: Sized;

    fn equal_to(&self, other: &dyn Packet) -> bool;
    fn clone_to(&self) -> Box<dyn Packet>;
    fn as_any(&self) -> &dyn std::any::Any;
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

/// Marshal takes an array of Packets and serializes them to a single buffer
pub fn marshal(packets: &[Box<dyn Packet>]) -> Result<Bytes, Error> {
    let mut out = BytesMut::new();

    for packet in packets {
        let a = packet.marshal()?;

        out.extend(a);
    }

    Ok(out.freeze())
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
        /*TODO:
        PacketType::TransportSpecificFeedback => match h.count {
            header::FORMAT_TLN => Box::new(transport_layer_nack::TransportLayerNack::default()),

            header::FORMAT_RRR => {
                Box::new(rapid_resynchronization_request::RapidResynchronizationRequest::default())
            }

            header::FORMAT_TCC => Box::new(transport_layer_cc::TransportLayerCc::default()),

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

            header::FORMAT_FIR => Box::new(full_intra_request::FullIntraRequest::default()),

            _ => Box::new(RawPacket::default()),
        },*/
        _ => Box::new(RawPacket::unmarshal(&in_packet)?),
    };

    Ok((p, bytes_processed))
}
/*
#[cfg(test)]
mod test {
    use crate::{
        error::Error, goodbye, packet::*, picture_loss_indication, rapid_resynchronization_request,
        reception_report, source_description,
    };

    const BYTES: [u8; 116] = [
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
        0x7b, 0x39, 0x63, 0x30, 0x30, 0x65, 0x62, 0x39, 0x32, 0x2d, 0x31, 0x61, 0x66, 0x62, 0x2d,
        0x39, 0x64, 0x34, 0x39, 0x2d, 0x61, 0x34, 0x37, 0x64, 0x2d, 0x39, 0x31, 0x66, 0x36, 0x34,
        0x65, 0x65, 0x65, 0x36, 0x39, 0x66, 0x35,
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
    ];

    #[test]
    fn test_packet_unmarshal() {
        let packet = unmarshal(BYTES[..].into()).expect("Error unmarshalling packets");

        let a = receiver_report::ReceiverReport {
            ssrc: 0x902f9e2e,
            reports: vec![reception_report::ReceptionReport {
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

        let b = source_description::SourceDescription {
            chunks: vec![source_description::SourceDescriptionChunk {
                source: 0x902f9e2e,
                items: vec![source_description::SourceDescriptionItem {
                    sdes_type: source_description::SdesType::SdesCname,
                    text: "{9c00eb92-1afb-9d49-a47d-91f64eee69f5}".to_string(),
                }],
            }],
        };

        let c = goodbye::Goodbye {
            sources: vec![0x902f9e2e],
            ..Default::default()
        };

        let d = picture_loss_indication::PictureLossIndication {
            sender_ssrc: 0x902f9e2e,
            media_ssrc: 0x902f9e2e,
        };

        let e = rapid_resynchronization_request::RapidResynchronizationRequest {
            sender_ssrc: 0x902f9e2e,
            media_ssrc: 0x902f9e2e,
        };

        let expected: Vec<Box<dyn Packet>> = vec![
            Box::new(a),
            Box::new(b),
            Box::new(c),
            Box::new(d),
            Box::new(e),
        ];

        assert_eq!(packet.len(), 5);

        if packet != expected {
            panic!("Invalid packets")
        }
    }

    #[test]
    fn test_packet_unmarshal_empty() -> Result<(), Error> {
        let result = unmarshal(BytesMut::new());
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
        let data: [u8; 4] = [
            // Goodbye (offset=84)
            // v=2, p=0, count=1, BYE, len=100
            0x81, 0xcb, 0x0, 0x64,
        ];

        let result = unmarshal(data[..].into());
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
*/
