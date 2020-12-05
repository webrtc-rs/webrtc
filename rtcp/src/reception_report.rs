use std::io::{Read, Write};

use util::Error;

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use super::errors::*;

// A ReceptionReport block conveys statistics on the reception of RTP packets
// from a single synchronization source.
#[derive(Debug, PartialEq, Default, Clone)]
pub struct ReceptionReport {
    // The SSRC identifier of the source to which the information in this
    // reception report block pertains.
    pub ssrc: u32,
    // The fraction of RTP data packets from source SSRC lost since the
    // previous SR or RR packet was sent, expressed as a fixed point
    // number with the binary point at the left edge of the field.
    pub fraction_lost: u8,
    // The total number of RTP data packets from source SSRC that have
    // been lost since the beginning of reception.
    pub total_lost: u32,
    // The low 16 bits contain the highest sequence number received in an
    // RTP data packet from source SSRC, and the most significant 16
    // bits extend that sequence number with the corresponding count of
    // sequence number cycles.
    pub last_sequence_number: u32,
    // An estimate of the statistical variance of the RTP data packet
    // interarrival time, measured in timestamp units and expressed as an
    // unsigned integer.
    pub jitter: u32,
    // The middle 32 bits out of 64 in the NTP timestamp received as part of
    // the most recent RTCP sender report (SR) packet from source SSRC. If no
    // SR has been received yet, the field is set to zero.
    pub last_sender_report: u32,
    // The delay, expressed in units of 1/65536 seconds, between receiving the
    // last SR packet from source SSRC and sending this reception report block.
    // If no SR packet has been received yet from SSRC, the field is set to zero.
    pub delay: u32,
}

const RECEPTION_REPORT_LENGTH: usize = 24;
/*const fractionLostOffset: u8 = 4;
const totalLostOffset: u8 = 5;
const lastSeqOffset: u8 = 8;
const jitterOffset: u8 = 12;
const lastSROffset: u8 = 16;
const delayOffset: u8 = 20;*/

impl ReceptionReport {
    pub fn size(&self) -> usize {
        RECEPTION_REPORT_LENGTH
    }

    // Marshal encodes the ReceptionReport in binary
    pub fn marshal<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
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
        writer.write_u32::<BigEndian>(self.ssrc)?;
        writer.write_u8(self.fraction_lost)?;

        // pack TotalLost into 24 bits
        if self.total_lost >= (1 << 25) {
            return Err(ERR_INVALID_TOTAL_LOST.clone());
        }
        writer.write_u8(((self.total_lost >> 16) & 0xFF) as u8)?;
        writer.write_u8(((self.total_lost >> 8) & 0xFF) as u8)?;
        writer.write_u8(((self.total_lost) & 0xFF) as u8)?;

        writer.write_u32::<BigEndian>(self.last_sequence_number)?;
        writer.write_u32::<BigEndian>(self.jitter)?;
        writer.write_u32::<BigEndian>(self.last_sender_report)?;
        writer.write_u32::<BigEndian>(self.delay)?;

        Ok(writer.flush()?)
    }

    // Unmarshal decodes the ReceptionReport from binary
    pub fn unmarshal<R: Read>(reader: &mut R) -> Result<Self, Error> {
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

        let ssrc = reader.read_u32::<BigEndian>()?;
        let fraction_lost = reader.read_u8()?;

        let b0 = reader.read_u8()?;
        let b1 = reader.read_u8()?;
        let b2 = reader.read_u8()?;
        let total_lost = b2 as u32 | (b1 as u32) << 8 | (b0 as u32) << 16;

        let last_sequence_number = reader.read_u32::<BigEndian>()?;
        let jitter = reader.read_u32::<BigEndian>()?;
        let last_sender_report = reader.read_u32::<BigEndian>()?;
        let delay = reader.read_u32::<BigEndian>()?;

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
