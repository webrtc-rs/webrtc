use byteorder::{BigEndian, ByteOrder};
use bytes::BytesMut;
use header::{Header, PacketType};
use std::fmt;

use crate::{errors::Error, packet::Packet, reception_report::ReceptionReport};
use crate::{header, util::get_padding};

// A SenderReport (SR) packet provides reception quality feedback for an RTP stream
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
    pub profile_extensions: Vec<u8>,
}

impl Packet for SenderReport {
    // Unmarshal decodes the ReceptionReport from binary
    fn unmarshal(&mut self, mut raw_packet: &mut BytesMut) -> Result<(), Error> {
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

        if raw_packet.len() < (header::HEADER_LENGTH + super::SR_HEADER_LENGTH) {
            return Err(Error::PacketTooShort);
        }

        let mut header = Header::default();

        header.unmarshal(&mut raw_packet)?;

        if header.packet_type != PacketType::SenderReport {
            return Err(Error::WrongType);
        }

        let packet_body = &raw_packet[header::HEADER_LENGTH..];

        self.ssrc = BigEndian::read_u32(&packet_body[super::SR_SSRC_OFFSET..]);
        self.ntp_time = BigEndian::read_u64(&packet_body[super::SR_NTP_OFFSET..]);
        self.rtp_time = BigEndian::read_u32(&packet_body[super::SR_RTP_OFFSET..]);
        self.packet_count = BigEndian::read_u32(&packet_body[super::SR_PACKET_COUNT_OFFSET..]);
        self.octet_count = BigEndian::read_u32(&packet_body[super::SR_OCTET_COUNT_OFFSET..]);

        let mut offset = super::SR_REPORT_OFFSET;

        for _ in 0..header.count {
            let rr_end = offset + crate::reception_report::RECEPTION_REPORT_LENGTH;

            if rr_end > packet_body.len() {
                return Err(Error::PacketTooShort);
            }

            let mut rr_body = packet_body
                [offset..offset + crate::reception_report::RECEPTION_REPORT_LENGTH]
                .into();

            offset = rr_end;

            let mut reception_report = ReceptionReport::default();

            reception_report.unmarshal(&mut rr_body)?;
            self.reports.push(reception_report);
        }

        if offset < packet_body.len() {
            self.profile_extensions = packet_body[offset..].to_vec();
        }

        if self.reports.len() as u8 != header.count {
            return Err(Error::InvalidHeader);
        }

        Ok(())
    }

    // Marshal encodes the packet in binary.
    fn marshal(&self) -> Result<BytesMut, Error> {
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

        let mut raw_packet = vec![0u8; self.len()];
        let packet_body = &mut raw_packet[header::HEADER_LENGTH..];

        BigEndian::write_u32(&mut packet_body[super::SR_SSRC_OFFSET..], self.ssrc);
        BigEndian::write_u64(&mut packet_body[super::SR_NTP_OFFSET..], self.ntp_time);
        BigEndian::write_u32(&mut packet_body[super::SR_RTP_OFFSET..], self.rtp_time);
        BigEndian::write_u32(
            &mut packet_body[super::SR_PACKET_COUNT_OFFSET..],
            self.packet_count,
        );
        BigEndian::write_u32(
            &mut packet_body[super::SR_OCTET_COUNT_OFFSET..],
            self.octet_count,
        );

        let mut offset = super::SR_HEADER_LENGTH;

        for rp in &self.reports {
            let data = rp.marshal()?;

            packet_body[offset..offset + data.len()].copy_from_slice(&data);

            offset += crate::reception_report::RECEPTION_REPORT_LENGTH;
        }

        if self.reports.len() > header::COUNT_MAX {
            return Err(Error::TooManyReports);
        }

        packet_body[offset..offset + self.profile_extensions.len()]
            .copy_from_slice(&self.profile_extensions);

        let header_data = self.header().marshal()?;

        raw_packet[..header_data.len()].copy_from_slice(&header_data);

        Ok(raw_packet[..].into())
    }

    /// destination_ssrc returns an array of SSRC values that this packet refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        let mut out: Vec<u32> = self.reports.iter().map(|x| x.ssrc).collect();
        out.push(self.ssrc);
        out
    }

    fn trait_eq(&self, other: &dyn Packet) -> bool {
        other
            .as_any()
            .downcast_ref::<SenderReport>()
            .map_or(false, |a| self == a)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl SenderReport {
    fn len(&self) -> usize {
        let mut reps_length = 0;
        for rep in &self.reports {
            reps_length += rep.len();
        }

        header::HEADER_LENGTH
            + super::SR_HEADER_LENGTH
            + reps_length
            + self.profile_extensions.len()
    }

    // Header returns the Header associated with this packet.
    pub fn header(&self) -> Header {
        let l = self.len() + get_padding(self.len());
        Header {
            padding: get_padding(self.len()) != 0,
            count: self.reports.len() as u8,
            packet_type: PacketType::SenderReport,
            length: ((l / 4) - 1) as u16,
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
