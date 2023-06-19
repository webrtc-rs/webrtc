use std::any::Any;
use std::fmt;

use bytes::{Buf, BufMut};
use util::marshal::{Marshal, MarshalSize, Unmarshal};

use crate::error::Error;
use crate::header::*;
use crate::packet::*;
use crate::util::*;

pub(crate) const RECEPTION_REPORT_LENGTH: usize = 24;
pub(crate) const FRACTION_LOST_OFFSET: usize = 4;
pub(crate) const TOTAL_LOST_OFFSET: usize = 5;
pub(crate) const LAST_SEQ_OFFSET: usize = 8;
pub(crate) const JITTER_OFFSET: usize = 12;
pub(crate) const LAST_SR_OFFSET: usize = 16;
pub(crate) const DELAY_OFFSET: usize = 20;

/// A ReceptionReport block conveys statistics on the reception of RTP packets
/// from a single synchronization source.
#[derive(Debug, PartialEq, Eq, Default, Clone)]
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
    /// The least significant 16 bits contain the highest sequence number received
    /// in an RTP data packet from source SSRC, and the most significant 16 bits extend
    /// that sequence number with the corresponding count of sequence number cycles.
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

impl fmt::Display for ReceptionReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl Packet for ReceptionReport {
    fn header(&self) -> Header {
        Header::default()
    }

    fn destination_ssrc(&self) -> Vec<u32> {
        vec![]
    }

    fn raw_size(&self) -> usize {
        RECEPTION_REPORT_LENGTH
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }

    fn equal(&self, other: &(dyn Packet + Send + Sync)) -> bool {
        other
            .as_any()
            .downcast_ref::<ReceptionReport>()
            .map_or(false, |a| self == a)
    }

    fn cloned(&self) -> Box<dyn Packet + Send + Sync> {
        Box::new(self.clone())
    }
}

impl MarshalSize for ReceptionReport {
    fn marshal_size(&self) -> usize {
        let l = self.raw_size();
        // align to 32-bit boundary
        l + get_padding_size(l)
    }
}

impl Marshal for ReceptionReport {
    /// marshal_to encodes the ReceptionReport in binary
    fn marshal_to(&self, mut buf: &mut [u8]) -> Result<usize, util::Error> {
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
        if buf.remaining_mut() < self.marshal_size() {
            return Err(Error::BufferTooShort.into());
        }

        buf.put_u32(self.ssrc);

        buf.put_u8(self.fraction_lost);

        // pack TotalLost into 24 bits
        if self.total_lost >= (1 << 25) {
            return Err(Error::InvalidTotalLost.into());
        }

        buf.put_u8(((self.total_lost >> 16) & 0xFF) as u8);
        buf.put_u8(((self.total_lost >> 8) & 0xFF) as u8);
        buf.put_u8((self.total_lost & 0xFF) as u8);

        buf.put_u32(self.last_sequence_number);
        buf.put_u32(self.jitter);
        buf.put_u32(self.last_sender_report);
        buf.put_u32(self.delay);

        put_padding(buf, self.raw_size());

        Ok(self.marshal_size())
    }
}

impl Unmarshal for ReceptionReport {
    /// unmarshal decodes the ReceptionReport from binary
    fn unmarshal<B>(raw_packet: &mut B) -> Result<Self, util::Error>
    where
        Self: Sized,
        B: Buf,
    {
        let raw_packet_len = raw_packet.remaining();
        if raw_packet_len < RECEPTION_REPORT_LENGTH {
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
        let ssrc = raw_packet.get_u32();
        let fraction_lost = raw_packet.get_u8();

        let t0 = raw_packet.get_u8();
        let t1 = raw_packet.get_u8();
        let t2 = raw_packet.get_u8();
        // TODO: The type of `total_lost` should be `i32`, per the RFC:
        // The total number of RTP data packets from source SSRC_n that have
        // been lost since the beginning of reception.  This number is
        // defined to be the number of packets expected less the number of
        // packets actually received, where the number of packets received
        // includes any which are late or duplicates.  Thus, packets that
        // arrive late are not counted as lost, and the loss may be negative
        // if there are duplicates.  The number of packets expected is
        // defined to be the extended last sequence number received, as
        // defined next, less the initial sequence number received.  This may
        // be calculated as shown in Appendix A.3.
        let total_lost = (t2 as u32) | (t1 as u32) << 8 | (t0 as u32) << 16;

        let last_sequence_number = raw_packet.get_u32();
        let jitter = raw_packet.get_u32();
        let last_sender_report = raw_packet.get_u32();
        let delay = raw_packet.get_u32();

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
}
