#[cfg(test)]
mod receiver_report_test;

use std::any::Any;
use std::fmt;

use bytes::{Buf, BufMut, Bytes};
use util::marshal::{Marshal, MarshalSize, Unmarshal};

use crate::error::Error;
use crate::header::*;
use crate::packet::*;
use crate::reception_report::*;
use crate::util::*;

type Result<T> = std::result::Result<T, util::Error>;

pub(super) const RR_SSRC_OFFSET: usize = HEADER_LENGTH;
pub(super) const RR_REPORT_OFFSET: usize = RR_SSRC_OFFSET + SSRC_LENGTH;

/// A ReceiverReport (RR) packet provides reception quality feedback for an RTP stream
#[derive(Debug, PartialEq, Eq, Default, Clone)]
pub struct ReceiverReport {
    /// The synchronization source identifier for the originator of this RR packet.
    pub ssrc: u32,
    /// Zero or more reception report blocks depending on the number of other
    /// sources heard by this sender since the last report. Each reception report
    /// block conveys statistics on the reception of RTP packets from a
    /// single synchronization source.
    pub reports: Vec<ReceptionReport>,
    /// Extension contains additional, payload-specific information that needs to
    /// be reported regularly about the receiver.
    pub profile_extensions: Bytes,
}

impl fmt::Display for ReceiverReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = format!("ReceiverReport from {}\n", self.ssrc);
        out += "\tSSRC    \tLost\tLastSequence\n";
        for rep in &self.reports {
            out += format!(
                "\t{:x}\t{}/{}\t{}\n",
                rep.ssrc, rep.fraction_lost, rep.total_lost, rep.last_sequence_number
            )
            .as_str();
        }
        out += format!("\tProfile Extension Data: {:?}\n", self.profile_extensions).as_str();

        write!(f, "{out}")
    }
}

impl Packet for ReceiverReport {
    /// Header returns the Header associated with this packet.
    fn header(&self) -> Header {
        Header {
            padding: get_padding_size(self.raw_size()) != 0,
            count: self.reports.len() as u8,
            packet_type: PacketType::ReceiverReport,
            length: ((self.marshal_size() / 4) - 1) as u16,
        }
    }

    /// destination_ssrc returns an array of SSRC values that this packet refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        self.reports.iter().map(|x| x.ssrc).collect()
    }

    fn raw_size(&self) -> usize {
        let mut reps_length = 0;
        for rep in &self.reports {
            reps_length += rep.marshal_size();
        }

        HEADER_LENGTH + SSRC_LENGTH + reps_length + self.profile_extensions.len()
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }

    fn equal(&self, other: &(dyn Packet + Send + Sync)) -> bool {
        other
            .as_any()
            .downcast_ref::<ReceiverReport>()
            .map_or(false, |a| self == a)
    }

    fn cloned(&self) -> Box<dyn Packet + Send + Sync> {
        Box::new(self.clone())
    }
}

impl MarshalSize for ReceiverReport {
    fn marshal_size(&self) -> usize {
        let l = self.raw_size();
        // align to 32-bit boundary
        l + get_padding_size(l)
    }
}

impl Marshal for ReceiverReport {
    /// marshal_to encodes the packet in binary.
    fn marshal_to(&self, mut buf: &mut [u8]) -> Result<usize> {
        if self.reports.len() > COUNT_MAX {
            return Err(Error::TooManyReports.into());
        }

        if buf.remaining_mut() < self.marshal_size() {
            return Err(Error::BufferTooShort.into());
        }

        /*
         *         0                   1                   2                   3
         *         0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
         *        +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         * header |V=2|P|    RC   |   PT=RR=201   |             length            |
         *        +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         *        |                     SSRC of packet sender                     |
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
        let h = self.header();
        let n = h.marshal_to(buf)?;
        buf = &mut buf[n..];

        buf.put_u32(self.ssrc);

        for report in &self.reports {
            let n = report.marshal_to(buf)?;
            buf = &mut buf[n..];
        }

        buf.put(self.profile_extensions.clone());

        if h.padding {
            put_padding(buf, self.raw_size());
        }

        Ok(self.marshal_size())
    }
}

impl Unmarshal for ReceiverReport {
    /// Unmarshal decodes the ReceiverReport from binary
    fn unmarshal<B>(raw_packet: &mut B) -> Result<Self>
    where
        Self: Sized,
        B: Buf,
    {
        /*
         *         0                   1                   2                   3
         *         0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
         *        +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         * header |V=2|P|    RC   |   PT=RR=201   |             length            |
         *        +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         *        |                     SSRC of packet sender                     |
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
        let raw_packet_len = raw_packet.remaining();
        if raw_packet_len < (HEADER_LENGTH + SSRC_LENGTH) {
            return Err(Error::PacketTooShort.into());
        }

        let header = Header::unmarshal(raw_packet)?;
        if header.packet_type != PacketType::ReceiverReport {
            return Err(Error::WrongType.into());
        }

        let ssrc = raw_packet.get_u32();

        let mut offset = RR_REPORT_OFFSET;
        let mut reports = Vec::with_capacity(header.count as usize);
        for _ in 0..header.count {
            if offset + RECEPTION_REPORT_LENGTH > raw_packet_len {
                return Err(Error::PacketTooShort.into());
            }
            let reception_report = ReceptionReport::unmarshal(raw_packet)?;
            reports.push(reception_report);
            offset += RECEPTION_REPORT_LENGTH;
        }
        let profile_extensions = raw_packet.copy_to_bytes(raw_packet.remaining());
        /*
        if header.padding && raw_packet.has_remaining() {
            raw_packet.advance(raw_packet.remaining());
        }
         */

        Ok(ReceiverReport {
            ssrc,
            reports,
            profile_extensions,
        })
    }
}
