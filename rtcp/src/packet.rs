use std::any::Any;
use std::fmt;

use bytes::{Buf, BufMut, Bytes, BytesMut};
use util::marshal::{Marshal, Unmarshal};

use crate::error::{Error, Result};
use crate::extended_report::ExtendedReport;
use crate::goodbye::*;
use crate::header::*;
use crate::payload_feedbacks::full_intra_request::*;
use crate::payload_feedbacks::picture_loss_indication::*;
use crate::payload_feedbacks::receiver_estimated_maximum_bitrate::*;
use crate::payload_feedbacks::slice_loss_indication::*;
use crate::raw_packet::*;
use crate::receiver_report::*;
use crate::sender_report::*;
use crate::source_description::*;
use crate::transport_feedbacks::rapid_resynchronization_request::*;
use crate::transport_feedbacks::transport_layer_cc::*;
use crate::transport_feedbacks::transport_layer_nack::*;

/// Packet represents an RTCP packet, a protocol used for out-of-band statistics and
/// control information for an RTP session
pub trait Packet: Marshal + Unmarshal + fmt::Display + fmt::Debug {
    fn header(&self) -> Header;
    fn destination_ssrc(&self) -> Vec<u32>;
    fn raw_size(&self) -> usize;
    fn as_any(&self) -> &(dyn Any + Send + Sync);
    fn equal(&self, other: &(dyn Packet + Send + Sync)) -> bool;
    fn cloned(&self) -> Box<dyn Packet + Send + Sync>;
}

impl PartialEq for dyn Packet + Send + Sync {
    fn eq(&self, other: &Self) -> bool {
        self.equal(other)
    }
}

impl Clone for Box<dyn Packet + Send + Sync> {
    fn clone(&self) -> Box<dyn Packet + Send + Sync> {
        self.cloned()
    }
}

/// marshal takes an array of Packets and serializes them to a single buffer
pub fn marshal(packets: &[Box<dyn Packet + Send + Sync>]) -> Result<Bytes> {
    let mut out = BytesMut::new();
    for p in packets {
        let data = p.marshal()?;
        out.put(data);
    }
    Ok(out.freeze())
}

/// Unmarshal takes an entire udp datagram (which may consist of multiple RTCP packets) and
/// returns the unmarshaled packets it contains.
///
/// If this is a reduced-size RTCP packet a feedback packet (Goodbye, SliceLossIndication, etc)
/// will be returned. Otherwise, the underlying type of the returned packet will be
/// CompoundPacket.
pub fn unmarshal<B>(raw_data: &mut B) -> Result<Vec<Box<dyn Packet + Send + Sync>>>
where
    B: Buf,
{
    let mut packets = vec![];

    while raw_data.has_remaining() {
        let p = unmarshaller(raw_data)?;
        packets.push(p);
    }

    match packets.len() {
        // Empty Packet
        0 => Err(Error::InvalidHeader),

        // Multiple Packet
        _ => Ok(packets),
    }
}

/// unmarshaller is a factory which pulls the first RTCP packet from a bytestream,
/// and returns it's parsed representation, and the amount of data that was processed.
pub(crate) fn unmarshaller<B>(raw_data: &mut B) -> Result<Box<dyn Packet + Send + Sync>>
where
    B: Buf,
{
    let h = Header::unmarshal(raw_data)?;

    let length = (h.length as usize) * 4;
    if length > raw_data.remaining() {
        return Err(Error::PacketTooShort);
    }

    let mut in_packet = h.marshal()?.chain(raw_data.take(length));

    let p: Box<dyn Packet + Send + Sync> = match h.packet_type {
        PacketType::SenderReport => Box::new(SenderReport::unmarshal(&mut in_packet)?),
        PacketType::ReceiverReport => Box::new(ReceiverReport::unmarshal(&mut in_packet)?),
        PacketType::SourceDescription => Box::new(SourceDescription::unmarshal(&mut in_packet)?),
        PacketType::Goodbye => Box::new(Goodbye::unmarshal(&mut in_packet)?),

        PacketType::TransportSpecificFeedback => match h.count {
            FORMAT_TLN => Box::new(TransportLayerNack::unmarshal(&mut in_packet)?),
            FORMAT_RRR => Box::new(RapidResynchronizationRequest::unmarshal(&mut in_packet)?),
            FORMAT_TCC => Box::new(TransportLayerCc::unmarshal(&mut in_packet)?),
            _ => Box::new(RawPacket::unmarshal(&mut in_packet)?),
        },
        PacketType::PayloadSpecificFeedback => match h.count {
            FORMAT_PLI => Box::new(PictureLossIndication::unmarshal(&mut in_packet)?),
            FORMAT_SLI => Box::new(SliceLossIndication::unmarshal(&mut in_packet)?),
            FORMAT_REMB => Box::new(ReceiverEstimatedMaximumBitrate::unmarshal(&mut in_packet)?),
            FORMAT_FIR => Box::new(FullIntraRequest::unmarshal(&mut in_packet)?),
            _ => Box::new(RawPacket::unmarshal(&mut in_packet)?),
        },
        PacketType::ExtendedReport => Box::new(ExtendedReport::unmarshal(&mut in_packet)?),
        _ => Box::new(RawPacket::unmarshal(&mut in_packet)?),
    };

    Ok(p)
}

#[cfg(test)]
mod test {
    use bytes::Bytes;

    use super::*;
    use crate::reception_report::*;

    #[test]
    fn test_packet_unmarshal() {
        let mut data = Bytes::from_static(&[
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

        let packet = unmarshal(&mut data).expect("Error unmarshalling packets");

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

        let expected: Vec<Box<dyn Packet + Send + Sync>> = vec![
            Box::new(a),
            Box::new(b),
            Box::new(c),
            Box::new(d),
            Box::new(e),
        ];

        assert!(packet == expected, "Invalid packets");
    }

    #[test]
    fn test_packet_unmarshal_empty() -> Result<()> {
        let result = unmarshal(&mut Bytes::new());
        if let Err(got) = result {
            let want = Error::InvalidHeader;
            assert_eq!(got, want, "Unmarshal(nil) err = {got}, want {want}");
        } else {
            panic!("want error");
        }

        Ok(())
    }

    #[test]
    fn test_packet_invalid_header_length() -> Result<()> {
        let mut data = Bytes::from_static(&[
            // Goodbye (offset=84)
            // v=2, p=0, count=1, BYE, len=100
            0x81, 0xcb, 0x0, 0x64,
        ]);

        let result = unmarshal(&mut data);
        if let Err(got) = result {
            let want = Error::PacketTooShort;
            assert_eq!(
                got, want,
                "Unmarshal(invalid_header_length) err = {got}, want {want}"
            );
        } else {
            panic!("want error");
        }

        Ok(())
    }
    #[test]
    fn test_packet_unmarshal_firefox() -> Result<()> {
        // issue report from https://github.com/webrtc-rs/srtp/issues/7
        let tests = vec![
            Bytes::from_static(&[
                143, 205, 0, 6, 65, 227, 184, 49, 118, 243, 78, 96, 42, 63, 0, 5, 12, 162, 166, 0,
                32, 5, 200, 4, 0, 4, 0, 0,
            ]),
            Bytes::from_static(&[
                143, 205, 0, 9, 65, 227, 184, 49, 118, 243, 78, 96, 42, 68, 0, 17, 12, 162, 167, 1,
                32, 17, 88, 0, 4, 0, 4, 8, 108, 0, 4, 0, 4, 12, 0, 4, 0, 4, 4, 0,
            ]),
            Bytes::from_static(&[
                143, 205, 0, 8, 65, 227, 184, 49, 118, 243, 78, 96, 42, 91, 0, 12, 12, 162, 168, 3,
                32, 12, 220, 4, 0, 4, 0, 8, 128, 4, 0, 4, 0, 8, 0, 0,
            ]),
            Bytes::from_static(&[
                143, 205, 0, 7, 65, 227, 184, 49, 118, 243, 78, 96, 42, 103, 0, 8, 12, 162, 169, 4,
                32, 8, 232, 4, 0, 4, 0, 4, 4, 0, 0, 0,
            ]),
        ];

        for mut test in tests {
            unmarshal(&mut test)?;
        }

        Ok(())
    }
}
