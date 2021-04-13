#[cfg(test)]
mod compound_packet_test;

use crate::{error::Error, packet::*, receiver_report::*, sender_report::*, source_description::*};

use bytes::{Bytes, BytesMut};
use std::any::Any;

/// A CompoundPacket is a collection of RTCP packets transmitted as a single packet with
/// the underlying protocol (for example UDP).
///
/// To maximize the resolution of receiption statistics, the first Packet in a CompoundPacket
/// must always be either a SenderReport or a ReceiverReport.  This is true even if no data
/// has been sent or received, in which case an empty ReceiverReport must be sent, and even
/// if the only other RTCP packet in the compound packet is a Goodbye.
///
/// Next, a SourceDescription containing a CNAME item must be included in each CompoundPacket
/// to identify the source and to begin associating media for purposes such as lip-sync.
///
/// Other RTCP packet types may follow in any order. Packet types may appear more than once.
#[derive(PartialEq, Default, Clone)]
pub struct CompoundPacket(pub Vec<Box<dyn Packet>>);

impl Packet for CompoundPacket {
    /// destination_ssrc returns the synchronization sources associated with this
    /// CompoundPacket's reception report.
    fn destination_ssrc(&self) -> Vec<u32> {
        if self.0.is_empty() {
            vec![]
        } else {
            self.0[0].destination_ssrc()
        }
    }

    fn marshal_size(&self) -> usize {
        let mut l = 0;
        for packet in &self.0 {
            l += packet.marshal_size();
        }
        l
    }

    /// Marshal encodes the CompoundPacket as binary.
    fn marshal(&self) -> Result<Bytes, Error> {
        self.validate()?;

        let mut out = BytesMut::new();
        for packet in &self.0 {
            let a = packet.marshal()?;
            out.extend(a);
        }
        Ok(out.freeze())
    }

    fn unmarshal(raw_data: &Bytes) -> Result<Self, Error> {
        let mut packets = vec![];

        let mut raw_data = raw_data.clone();
        while !raw_data.is_empty() {
            let (p, processed) = unmarshaller(&raw_data)?;
            packets.push(p);
            raw_data = raw_data.split_off(processed);
        }

        let c = CompoundPacket(packets);
        c.validate()?;

        Ok(c)
    }

    fn equal_to(&self, other: &dyn Packet) -> bool {
        other
            .as_any()
            .downcast_ref::<CompoundPacket>()
            .map_or(false, |a| self == a)
    }

    fn clone_to(&self) -> Box<dyn Packet> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl CompoundPacket {
    /// Validate returns an error if this is not an RFC-compliant CompoundPacket.
    pub fn validate(&self) -> Result<(), Error> {
        if self.0.is_empty() {
            return Err(Error::EmptyCompound);
        }

        // ToDo: Any way to match types cleanly???? @metaclips
        // ToDo: We need proper error handling. @metaclips
        // SenderReport and ReceiverReport are the only types that
        // are allowed to be the first packet in a compound datagram
        if self.0[0].as_any().downcast_ref::<SenderReport>().is_none()
            && self.0[0]
                .as_any()
                .downcast_ref::<ReceiverReport>()
                .is_none()
        {
            return Err(Error::BadFirstPacket);
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
                    return Err(Error::MissingCname);
                }

                return Ok(());

            // Other packets are not permitted before the CNAME
            } else {
                return Err(Error::PacketBeforeCname);
            }
        }

        // CNAME never reached
        Err(Error::MissingCname)
    }

    /// CNAME returns the CNAME that *must* be present in every CompoundPacket
    pub fn cname(&self) -> Result<Bytes, Error> {
        if self.0.is_empty() {
            return Err(Error::EmptyCompound);
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
                return Err(Error::PacketBeforeCname);
            }
        }

        Err(Error::MissingCname)
    }
}
