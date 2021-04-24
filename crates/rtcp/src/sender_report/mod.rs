#[cfg(test)]
mod sender_report_test;

use crate::{error::Error, header::*, packet::*, reception_report::*, util::*};

use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::any::Any;
use std::fmt;

pub(crate) const SR_HEADER_LENGTH: usize = 24;
pub(crate) const SR_SSRC_OFFSET: usize = HEADER_LENGTH;
pub(crate) const SR_REPORT_OFFSET: usize = SR_SSRC_OFFSET + SR_HEADER_LENGTH;

pub(crate) const SR_NTP_OFFSET: usize = SR_SSRC_OFFSET + SSRC_LENGTH;
pub(crate) const NTP_TIME_LENGTH: usize = 8;
pub(crate) const SR_RTP_OFFSET: usize = SR_NTP_OFFSET + NTP_TIME_LENGTH;
pub(crate) const RTP_TIME_LENGTH: usize = 4;
pub(crate) const SR_PACKET_COUNT_OFFSET: usize = SR_RTP_OFFSET + RTP_TIME_LENGTH;
pub(crate) const SR_PACKET_COUNT_LENGTH: usize = 4;
pub(crate) const SR_OCTET_COUNT_OFFSET: usize = SR_PACKET_COUNT_OFFSET + SR_PACKET_COUNT_LENGTH;
pub(crate) const SR_OCTET_COUNT_LENGTH: usize = 4;

/// A SenderReport (SR) packet provides reception quality feedback for an RTP stream
#[derive(Debug, PartialEq, Default, Clone)]
pub struct SenderReport {
    /// The synchronization source identifier for the originator of this SR packet.
    pub ssrc: u32,
    /// The wallclock time when this report was sent so that it may be used in
    /// combination with timestamps returned in reception reports from other
    /// receivers to measure round-trip propagation to those receivers.
    pub ntp_time: u64,
    /// Corresponds to the same time as the NTP timestamp (above), but in
    /// the same units and with the same random offset as the RTP
    /// timestamps in data packets. This correspondence may be used for
    /// intra- and inter-media synchronization for sources whose NTP
    /// timestamps are synchronized, and may be used by media-independent
    /// receivers to estimate the nominal RTP clock frequency.
    pub rtp_time: u32,
    /// The total number of RTP data packets transmitted by the sender
    /// since starting transmission up until the time this SR packet was
    /// generated.
    pub packet_count: u32,
    /// The total number of payload octets (i.e., not including header or
    /// padding) transmitted in RTP data packets by the sender since
    /// starting transmission up until the time this SR packet was
    /// generated.
    pub octet_count: u32,
    /// Zero or more reception report blocks depending on the number of other
    /// sources heard by this sender since the last report. Each reception report
    /// block conveys statistics on the reception of RTP packets from a
    /// single synchronization source.
    pub reports: Vec<ReceptionReport>,

    /// ProfileExtensions contains additional, payload-specific information that needs to
    /// be reported regularly about the sender.
    pub profile_extensions: Bytes,
}

impl Packet for SenderReport {
    /// destination_ssrc returns an array of SSRC values that this packet refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        let mut out: Vec<u32> = self.reports.iter().map(|x| x.ssrc).collect();
        out.push(self.ssrc);
        out
    }

    fn size(&self) -> usize {
        let mut reps_length = 0;
        for rep in &self.reports {
            reps_length += rep.marshal_size();
        }

        HEADER_LENGTH + SR_HEADER_LENGTH + reps_length + self.profile_extensions.len()
    }

    /// Marshal encodes the packet in binary.
    fn marshal(&self) -> Result<Bytes, Error> {
        if self.reports.len() > COUNT_MAX {
            return Err(Error::TooManyReports);
        }

        /*
         *         0                   1                   2                   3
         *         0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
         *        +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         * header |V=2|P|    RC   |   PT=SR=200   |             length            |
         *        +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         *        |                         SSRC of sender                        |
         *        +=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+
         * sender |              NTP timestamp, most significant word             |
         * info   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         *        |             NTP timestamp, least significant word             |
         *        +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         *        |                         RTP timestamp                         |
         *        +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         *        |                     sender's packet count                     |
         *        +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         *        |                      sender's octet count                     |
         *        +=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+
         * report |                 SSRC_1 (SSRC of first source)                 |
         * block  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         *   1    | fraction lost |       cumulative number of packets lost       |
         *        +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         *        |           extended highest sequence number received           |
         *        +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         *        |                      interarrival jitter                      |
         *        +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         *        |                         last SR (LSR)                         |
         *        +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         *        |                   delay since last SR (DLSR)                  |
         *        +=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+
         * report |                 SSRC_2 (SSRC of second source)                |
         * block  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         *   2    :                               ...                             :
         *        +=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+
         *        |                  profile-specific extensions                  |
         *        +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         */

        let mut writer = BytesMut::with_capacity(self.marshal_size());

        let h = self.header();
        let data = h.marshal()?;
        writer.extend(data);

        writer.put_u32(self.ssrc);
        writer.put_u64(self.ntp_time);
        writer.put_u32(self.rtp_time);
        writer.put_u32(self.packet_count);
        writer.put_u32(self.octet_count);

        for rp in &self.reports {
            let data = rp.marshal()?;
            writer.extend(data);
        }

        writer.extend(self.profile_extensions.clone());

        put_padding(&mut writer);
        Ok(writer.freeze())
    }

    /// Unmarshal decodes the ReceptionReport from binary
    fn unmarshal(raw_packet: &Bytes) -> Result<Self, Error> {
        /*
         *         0                   1                   2                   3
         *         0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
         *        +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         * header |V=2|P|    RC   |   PT=SR=200   |             length            |
         *        +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         *        |                         SSRC of sender                        |
         *        +=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+
         * sender |              NTP timestamp, most significant word             |
         * info   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         *        |             NTP timestamp, least significant word             |
         *        +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         *        |                         RTP timestamp                         |
         *        +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         *        |                     sender's packet count                     |
         *        +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         *        |                      sender's octet count                     |
         *        +=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+
         * report |                 SSRC_1 (SSRC of first source)                 |
         * block  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         *   1    | fraction lost |       cumulative number of packets lost       |
         *        +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         *        |           extended highest sequence number received           |
         *        +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         *        |                      interarrival jitter                      |
         *        +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         *        |                         last SR (LSR)                         |
         *        +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         *        |                   delay since last SR (DLSR)                  |
         *        +=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+
         * report |                 SSRC_2 (SSRC of second source)                |
         * block  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         *   2    :                               ...                             :
         *        +=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+
         *        |                  profile-specific extensions                  |
         *        +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         */

        if raw_packet.len() < (HEADER_LENGTH + SR_HEADER_LENGTH) {
            return Err(Error::PacketTooShort);
        }

        let header = Header::unmarshal(raw_packet)?;
        if header.packet_type != PacketType::SenderReport {
            return Err(Error::WrongType);
        }

        let reader = &mut raw_packet.slice(HEADER_LENGTH..);

        let ssrc = reader.get_u32();
        let ntp_time = reader.get_u64();
        let rtp_time = reader.get_u32();
        let packet_count = reader.get_u32();
        let octet_count = reader.get_u32();

        let mut offset = SR_REPORT_OFFSET;
        let mut reports = Vec::with_capacity(header.count as usize);
        for _ in 0..header.count {
            if offset + RECEPTION_REPORT_LENGTH > raw_packet.len() {
                return Err(Error::PacketTooShort);
            }
            let reception_report = ReceptionReport::unmarshal(&raw_packet.slice(offset..))?;
            reports.push(reception_report);
            offset += RECEPTION_REPORT_LENGTH;
        }
        let profile_extensions = raw_packet.slice(offset..);

        Ok(SenderReport {
            ssrc,
            ntp_time,
            rtp_time,
            packet_count,
            octet_count,
            reports,
            profile_extensions,
        })
    }

    fn equal_to(&self, other: &dyn Packet) -> bool {
        other
            .as_any()
            .downcast_ref::<SenderReport>()
            .map_or(false, |a| self == a)
    }

    fn clone_to(&self) -> Box<dyn Packet> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl SenderReport {
    /// Header returns the Header associated with this packet.
    pub fn header(&self) -> Header {
        Header {
            padding: get_padding(self.size()) != 0,
            count: self.reports.len() as u8,
            packet_type: PacketType::SenderReport,
            length: ((self.marshal_size() / 4) - 1) as u16,
        }
    }
}

impl fmt::Display for SenderReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = format!("SenderReport from {}\n", self.ssrc);
        out += format!("\tNTPTime:\t{}\n", self.ntp_time).as_str();
        out += format!("\tRTPTIme:\t{}\n", self.rtp_time).as_str();
        out += format!("\tPacketCount:\t{}\n", self.packet_count).as_str();
        out += format!("\tOctetCount:\t{}\n", self.octet_count).as_str();
        out += "\tSSRC    \tLost\tLastSequence\n";
        for rep in &self.reports {
            out += format!(
                "\t{:x}\t{}/{}\t{}\n",
                rep.ssrc, rep.fraction_lost, rep.total_lost, rep.last_sequence_number
            )
            .as_str();
        }
        out += format!("\tProfile Extension Data: {:?}\n", self.profile_extensions).as_str();

        write!(f, "{}", out)
    }
}
