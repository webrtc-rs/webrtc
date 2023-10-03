#[cfg(test)]
mod compound_packet_test;

use std::any::Any;
use std::fmt;

use bytes::{Buf, Bytes};
use util::marshal::{Marshal, MarshalSize, Unmarshal};

use crate::error::Error;
use crate::header::*;
use crate::packet::*;
use crate::receiver_report::*;
use crate::sender_report::*;
use crate::source_description::*;
use crate::util::*;

type Result<T> = std::result::Result<T, util::Error>;

/// A CompoundPacket is a collection of RTCP packets transmitted as a single packet with
/// the underlying protocol (for example UDP).
///
/// To maximize the resolution of reception statistics, the first Packet in a CompoundPacket
/// must always be either a SenderReport or a ReceiverReport.  This is true even if no data
/// has been sent or received, in which case an empty ReceiverReport must be sent, and even
/// if the only other RTCP packet in the compound packet is a Goodbye.
///
/// Next, a SourceDescription containing a CNAME item must be included in each CompoundPacket
/// to identify the source and to begin associating media for purposes such as lip-sync.
///
/// Other RTCP packet types may follow in any order. Packet types may appear more than once.
#[derive(Debug, Default, PartialEq, Clone)]
pub struct CompoundPacket(pub Vec<Box<dyn Packet + Send + Sync>>);

impl fmt::Display for CompoundPacket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl Packet for CompoundPacket {
    fn header(&self) -> Header {
        Header::default()
    }

    /// destination_ssrc returns the synchronization sources associated with this
    /// CompoundPacket's reception report.
    fn destination_ssrc(&self) -> Vec<u32> {
        if self.0.is_empty() {
            vec![]
        } else {
            self.0[0].destination_ssrc()
        }
    }

    fn raw_size(&self) -> usize {
        let mut l = 0;
        for packet in &self.0 {
            l += packet.marshal_size();
        }
        l
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }

    fn equal(&self, other: &(dyn Packet + Send + Sync)) -> bool {
        other
            .as_any()
            .downcast_ref::<CompoundPacket>()
            .map_or(false, |a| self == a)
    }

    fn cloned(&self) -> Box<dyn Packet + Send + Sync> {
        Box::new(self.clone())
    }
}

impl MarshalSize for CompoundPacket {
    fn marshal_size(&self) -> usize {
        let l = self.raw_size();
        // align to 32-bit boundary
        l + get_padding_size(l)
    }
}

impl Marshal for CompoundPacket {
    /// Marshal encodes the CompoundPacket as binary.
    fn marshal_to(&self, mut buf: &mut [u8]) -> Result<usize> {
        self.validate()?;

        for packet in &self.0 {
            let n = packet.marshal_to(buf)?;
            buf = &mut buf[n..];
        }

        Ok(self.marshal_size())
    }
}

impl Unmarshal for CompoundPacket {
    fn unmarshal<B>(raw_packet: &mut B) -> Result<Self>
    where
        Self: Sized,
        B: Buf,
    {
        let mut packets = vec![];

        while raw_packet.has_remaining() {
            let p = unmarshaller(raw_packet)?;
            packets.push(p);
        }

        let c = CompoundPacket(packets);
        c.validate()?;

        Ok(c)
    }
}

impl CompoundPacket {
    /// Validate returns an error if this is not an RFC-compliant CompoundPacket.
    pub fn validate(&self) -> Result<()> {
        if self.0.is_empty() {
            return Err(Error::EmptyCompound.into());
        }

        // SenderReport and ReceiverReport are the only types that
        // are allowed to be the first packet in a compound datagram
        if self.0[0].as_any().downcast_ref::<SenderReport>().is_none()
            && self.0[0]
                .as_any()
                .downcast_ref::<ReceiverReport>()
                .is_none()
        {
            return Err(Error::BadFirstPacket.into());
        }

        for pkt in &self.0[1..] {
            // If the number of RecetpionReports exceeds 31 additional ReceiverReports
            // can be included here.
            if pkt.as_any().downcast_ref::<ReceiverReport>().is_some() {
                continue;
            // A SourceDescription containing a CNAME must be included in every
            // CompoundPacket.
            } else if let Some(e) = pkt.as_any().downcast_ref::<SourceDescription>() {
                let mut has_cname = false;
                for c in &e.chunks {
                    for it in &c.items {
                        if it.sdes_type == SdesType::SdesCname {
                            has_cname = true
                        }
                    }
                }

                if !has_cname {
                    return Err(Error::MissingCname.into());
                }

                return Ok(());

            // Other packets are not permitted before the CNAME
            } else {
                return Err(Error::PacketBeforeCname.into());
            }
        }

        // CNAME never reached
        Err(Error::MissingCname.into())
    }

    /// CNAME returns the CNAME that *must* be present in every CompoundPacket
    pub fn cname(&self) -> Result<Bytes> {
        if self.0.is_empty() {
            return Err(Error::EmptyCompound.into());
        }

        for pkt in &self.0[1..] {
            if let Some(sdes) = pkt.as_any().downcast_ref::<SourceDescription>() {
                for c in &sdes.chunks {
                    for it in &c.items {
                        if it.sdes_type == SdesType::SdesCname {
                            return Ok(it.text.clone());
                        }
                    }
                }
            } else if pkt.as_any().downcast_ref::<ReceiverReport>().is_none() {
                return Err(Error::PacketBeforeCname.into());
            }
        }

        Err(Error::MissingCname.into())
    }
}
