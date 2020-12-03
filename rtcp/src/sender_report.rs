use std::fmt;
use std::io::{Read, Write};

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use util::Error;

use super::errors::*;
use super::header::*;
use super::reception_report::*;
use crate::util::get_padding;

#[cfg(test)]
mod sender_report_test;

// A SenderReport (SR) packet provides reception quality feedback for an RTP stream
#[derive(Debug, PartialEq, Default, Clone)]
pub struct SenderReport {
    // The synchronization source identifier for the originator of this SR packet.
    pub ssrc: u32,
    // The wallclock time when this report was sent so that it may be used in
    // combination with timestamps returned in reception reports from other
    // receivers to measure round-trip propagation to those receivers.
    pub ntp_time: u64,
    // Corresponds to the same time as the NTP timestamp (above), but in
    // the same units and with the same random offset as the RTP
    // timestamps in data packets. This correspondence may be used for
    // intra- and inter-media synchronization for sources whose NTP
    // timestamps are synchronized, and may be used by media-independent
    // receivers to estimate the nominal RTP clock frequency.
    pub rtp_time: u32,
    // The total number of RTP data packets transmitted by the sender
    // since starting transmission up until the time this SR packet was
    // generated.
    pub packet_count: u32,
    // The total number of payload octets (i.e., not including header or
    // padding) transmitted in RTP data packets by the sender since
    // starting transmission up until the time this SR packet was
    // generated.
    pub octet_count: u32,
    // Zero or more reception report blocks depending on the number of other
    // sources heard by this sender since the last report. Each reception report
    // block conveys statistics on the reception of RTP packets from a
    // single synchronization source.
    pub reports: Vec<ReceptionReport>,

    // ProfileExtensions contains additional, payload-specific information that needs to
    // be reported regularly about the sender.
    pub profile_extensions: Vec<u8>,
}

const SR_HEADER_LENGTH: usize = 24;
/*
const srSSRCOffset: usize = 0;
const srNTPOffset: usize = srSSRCOffset + SSRC_LENGTH;
const ntpTimeLength: usize = 8;
const srRTPOffset: usize = srNTPOffset + ntpTimeLength;
const rtpTimeLength: usize = 4;
const srPacketCountOffset: usize = srRTPOffset + rtpTimeLength;
const srPacketCountLength: usize = 4;
const srOctetCountOffset: usize = srPacketCountOffset + srPacketCountLength;
const srOctetCountLength: usize = 4;
const srReportOffset: usize = srOctetCountOffset + srOctetCountLength;*/

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

impl SenderReport {
    fn size(&self) -> usize {
        let mut reps_length = 0;
        for rep in &self.reports {
            reps_length += rep.size();
        }

        HEADER_LENGTH + SR_HEADER_LENGTH + reps_length + self.profile_extensions.len()
    }

    // Unmarshal decodes the ReceptionReport from binary
    pub fn unmarshal<R: Read>(reader: &mut R) -> Result<Self, Error> {
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
        let header = Header::unmarshal(reader)?;

        if header.packet_type != PacketType::SenderReport {
            return Err(ERR_WRONG_TYPE.clone());
        }

        let ssrc = reader.read_u32::<BigEndian>()?;
        let ntp_time = reader.read_u64::<BigEndian>()?;
        let rtp_time = reader.read_u32::<BigEndian>()?;
        let packet_count = reader.read_u32::<BigEndian>()?;
        let octet_count = reader.read_u32::<BigEndian>()?;

        let mut reports = vec![];
        for _i in 0..header.count {
            reports.push(ReceptionReport::unmarshal(reader)?);
        }

        let mut profile_extensions: Vec<u8> = vec![];
        reader.read_to_end(&mut profile_extensions)?;

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

    // Header returns the Header associated with this packet.
    pub fn header(&self) -> Header {
        let l = self.size() + get_padding(self.size());
        Header {
            padding: get_padding(self.size()) != 0,
            count: self.reports.len() as u8,
            packet_type: PacketType::SenderReport,
            length: ((l / 4) - 1) as u16,
        }
    }

    // destination_ssrc returns an array of SSRC values that this packet refers to.
    pub fn destination_ssrc(&self) -> Vec<u32> {
        let mut out: Vec<u32> = self.reports.iter().map(|x| x.ssrc).collect();
        out.push(self.ssrc);
        out
    }

    // Marshal encodes the packet in binary.
    pub fn marshal<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
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
        if self.reports.len() > COUNT_MAX {
            return Err(ERR_TOO_MANY_REPORTS.clone());
        }

        self.header().marshal(writer)?;

        writer.write_u32::<BigEndian>(self.ssrc)?;
        writer.write_u64::<BigEndian>(self.ntp_time)?;
        writer.write_u32::<BigEndian>(self.rtp_time)?;
        writer.write_u32::<BigEndian>(self.packet_count)?;
        writer.write_u32::<BigEndian>(self.octet_count)?;

        for rep in &self.reports {
            rep.marshal(writer)?;
        }

        writer.write_all(&self.profile_extensions)?;

        Ok(writer.flush()?)
    }
}
