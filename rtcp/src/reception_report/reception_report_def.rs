use crate::errors::*;

use byteorder::{BigEndian, ByteOrder};
use bytes::BytesMut;
use util::Error;

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

impl ReceptionReport {
    pub fn len(&self) -> usize {
        super::RECEPTION_REPORT_LENGTH
    }

    /// Marshal encodes the ReceptionReport in binary
    pub fn marshal(&self) -> Result<BytesMut, Error> {
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

        let mut raw_packet = vec![0u8; super::RECEPTION_REPORT_LENGTH];

        BigEndian::write_u32(&mut raw_packet, self.ssrc);

        raw_packet[super::FRACTION_LOST_OFFSET as usize] = self.fraction_lost;

        // pack TotalLost into 24 bits
        if self.total_lost >= (1 << 25) {
            return Err(ERR_INVALID_TOTAL_LOST.to_owned());
        }

        let tl_bytes = &mut raw_packet[super::TOTAL_LOST_OFFSET..];
        tl_bytes[0] = (self.total_lost >> 16) as u8;
        tl_bytes[1] = (self.total_lost >> 8) as u8;
        tl_bytes[2] = (self.total_lost) as u8;

        BigEndian::write_u32(
            &mut raw_packet[super::LAST_SEQ_OFFSET..],
            self.last_sequence_number,
        );

        BigEndian::write_u32(&mut raw_packet[super::JITTER_OFFSET..], self.jitter);

        BigEndian::write_u32(
            &mut raw_packet[super::LAST_SR_OFFSET..],
            self.last_sender_report,
        );

        BigEndian::write_u32(&mut raw_packet[super::DELAY_OFFSET..], self.delay);

        Ok(raw_packet[..].into())
    }

    /// Unmarshal decodes the ReceptionReport from binary
    pub fn unmarshal(&mut self, raw_packet: &mut BytesMut) -> Result<(), Error> {
        if raw_packet.len() < super::RECEPTION_REPORT_LENGTH {
            return Err(ERR_PACKET_TOO_SHORT.to_owned());
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

        self.ssrc = BigEndian::read_u32(raw_packet);
        self.fraction_lost = raw_packet[super::FRACTION_LOST_OFFSET];

        let tl_bytes = &mut raw_packet[super::TOTAL_LOST_OFFSET..];
        self.total_lost =
            (tl_bytes[2] as u32) | (tl_bytes[1] as u32) << 8 | (tl_bytes[0] as u32) << 16;

        self.last_sequence_number = BigEndian::read_u32(&raw_packet[super::LAST_SEQ_OFFSET..]);
        self.jitter = BigEndian::read_u32(&raw_packet[super::JITTER_OFFSET..]);
        self.last_sender_report = BigEndian::read_u32(&raw_packet[super::LAST_SR_OFFSET..]);
        self.delay = BigEndian::read_u32(&raw_packet[super::DELAY_OFFSET..]);

        Ok(())
    }
}
