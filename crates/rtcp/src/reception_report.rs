use crate::{error::Error, packet::*, util::*};

use anyhow::Result;
use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::any::Any;

pub(crate) const RECEPTION_REPORT_LENGTH: usize = 24;
pub(crate) const FRACTION_LOST_OFFSET: usize = 4;
pub(crate) const TOTAL_LOST_OFFSET: usize = 5;
pub(crate) const LAST_SEQ_OFFSET: usize = 8;
pub(crate) const JITTER_OFFSET: usize = 12;
pub(crate) const LAST_SR_OFFSET: usize = 16;
pub(crate) const DELAY_OFFSET: usize = 20;

/// A ReceptionReport block conveys statistics on the reception of RTP packets
/// from a single synchronization source.
#[derive(Debug, PartialEq, Default, Clone)]
pub struct ReceptionReport {
    /// The SSRC identifier of the source to which the information in this
    /// reception report block pertains.
    pub ssrc: u32,
    /// The fraction of RTP data packets from source SSRC lost since the
    /// previous SR or RR packet was sent, expressed as a fixed point
    /// number with the binary point at the left edge of the field.
    pub fraction_lost: u8,
    /// The total number of RTP data packets from source SSRC that have
    /// been lost since the beginning of reception.
    pub total_lost: u32,
    /// The low 16 bits contain the highest sequence number received in an
    /// RTP data packet from source SSRC, and the most significant 16
    /// bits extend that sequence number with the corresponding count of
    /// sequence number cycles.
    pub last_sequence_number: u32,
    /// An estimate of the statistical variance of the RTP data packet
    /// interarrival time, measured in timestamp units and expressed as an
    /// unsigned integer.
    pub jitter: u32,
    /// The middle 32 bits out of 64 in the NTP timestamp received as part of
    /// the most recent RTCP sender report (SR) packet from source SSRC. If no
    /// SR has been received yet, the field is set to zero.
    pub last_sender_report: u32,
    /// The delay, expressed in units of 1/65536 seconds, between receiving the
    /// last SR packet from source SSRC and sending this reception report block.
    /// If no SR packet has been received yet from SSRC, the field is set to zero.
    pub delay: u32,
}

impl Packet for ReceptionReport {
    fn destination_ssrc(&self) -> Vec<u32> {
        vec![]
    }

    fn size(&self) -> usize {
        RECEPTION_REPORT_LENGTH
    }

    /// Marshal encodes the ReceptionReport in binary
    fn marshal(&self) -> Result<Bytes> {
        /*
         *  0                   1                   2                   3
         *  0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
         * +=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+
         * |                              SSRC                             |
         * +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         * | fraction lost |       cumulative number of packets lost       |
         * +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         * |           extended highest sequence number received           |
         * +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         * |                      interarrival jitter                      |
         * +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         * |                         last SR (LSR)                         |
         * +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         * |                   delay since last SR (DLSR)                  |
         * +=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+
         */

        let mut writer = BytesMut::with_capacity(self.marshal_size());

        writer.put_u32(self.ssrc);

        writer.put_u8(self.fraction_lost);

        // pack TotalLost into 24 bits
        if self.total_lost >= (1 << 25) {
            return Err(Error::InvalidTotalLost.into());
        }

        writer.put_u8(((self.total_lost >> 16) & 0xFF) as u8);
        writer.put_u8(((self.total_lost >> 8) & 0xFF) as u8);
        writer.put_u8((self.total_lost & 0xFF) as u8);

        writer.put_u32(self.last_sequence_number);
        writer.put_u32(self.jitter);
        writer.put_u32(self.last_sender_report);
        writer.put_u32(self.delay);

        put_padding(&mut writer);
        Ok(writer.freeze())
    }

    /// Unmarshal decodes the ReceptionReport from binary
    fn unmarshal(raw_packet: &Bytes) -> Result<Self> {
        if raw_packet.len() < RECEPTION_REPORT_LENGTH {
            return Err(Error::PacketTooShort.into());
        }

        /*
         *  0                   1                   2                   3
         *  0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
         * +=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+
         * |                              SSRC                             |
         * +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         * | fraction lost |       cumulative number of packets lost       |
         * +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         * |           extended highest sequence number received           |
         * +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         * |                      interarrival jitter                      |
         * +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         * |                         last SR (LSR)                         |
         * +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         * |                   delay since last SR (DLSR)                  |
         * +=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+
         */

        let reader = &mut raw_packet.clone();

        let ssrc = reader.get_u32();
        let fraction_lost = reader.get_u8();

        let t0 = reader.get_u8();
        let t1 = reader.get_u8();
        let t2 = reader.get_u8();
        let total_lost = (t2 as u32) | (t1 as u32) << 8 | (t0 as u32) << 16;

        let last_sequence_number = reader.get_u32();
        let jitter = reader.get_u32();
        let last_sender_report = reader.get_u32();
        let delay = reader.get_u32();

        Ok(ReceptionReport {
            ssrc,
            fraction_lost,
            total_lost,
            last_sequence_number,
            jitter,
            last_sender_report,
            delay,
        })
    }

    fn equal(&self, other: &dyn Packet) -> bool {
        other
            .as_any()
            .downcast_ref::<ReceptionReport>()
            .map_or(false, |a| self == a)
    }

    fn cloned(&self) -> Box<dyn Packet> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
