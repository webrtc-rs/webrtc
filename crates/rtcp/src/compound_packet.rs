use std::io::Write;

use util::Error;

use super::errors::*;
use super::packet::*;
use super::source_description::*;

#[cfg(test)]
mod compound_packet_test;

// A CompoundPacket is a collection of RTCP packets transmitted as a single packet with
// the underlying protocol (for example UDP).
//
// To maximize the resolution of receiption statistics, the first Packet in a CompoundPacket
// must always be either a SenderReport or a ReceiverReport.  This is true even if no data
// has been sent or received, in which case an empty ReceiverReport must be sent, and even
// if the only other RTCP packet in the compound packet is a Goodbye.
//
// Next, a SourceDescription containing a CNAME item must be included in each CompoundPacket
// to identify the source and to begin associating media for purposes such as lip-sync.
//
// Other RTCP packet types may follow in any order. Packet types may appear more than once.
#[derive(Debug, Clone)]
pub struct CompoundPacket(pub Vec<Packet>);

impl CompoundPacket {
    // Validate returns an error if this is not an RFC-compliant CompoundPacket.
    pub fn validate(&self) -> Result<(), Error> {
        if self.0.is_empty() {
            return Err(ERR_EMPTY_COMPOUND.clone());
        }

        // SenderReport and ReceiverReport are the only types that
        // are allowed to be the first packet in a compound datagram
        match &self.0[0] {
            Packet::SenderReport(_) | Packet::ReceiverReport(_) => {}
            _ => return Err(ERR_BAD_FIRST_PACKET.clone()),
        };

        for pkt in &self.0[1..] {
            match pkt {
                // If the number of RecetpionReports exceeds 31 additional ReceiverReports
                // can be included here.
                Packet::ReceiverReport(_) => {}

                // A SourceDescription containing a CNAME must be included in every
                // CompoundPacket.
                Packet::SourceDescription(p) => {
                    let mut has_cname = false;
                    for c in &p.chunks {
                        for it in &c.items {
                            if it.sdes_type == SDESType::SDESCNAME {
                                has_cname = true
                            }
                        }
                    }

                    if !has_cname {
                        return Err(ERR_MISSING_CNAME.clone());
                    }

                    return Ok(());
                }
                // Other packets are not permitted before the CNAME
                _ => return Err(ERR_PACKET_BEFORE_CNAME.clone()),
            };
        }

        // CNAME never reached
        Err(ERR_MISSING_CNAME.clone())
    }

    //CNAME returns the CNAME that *must* be present in every CompoundPacket
    pub fn cname(&self) -> Result<String, Error> {
        if self.0.is_empty() {
            return Err(ERR_EMPTY_COMPOUND.clone());
        }

        for pkt in &self.0[1..] {
            match pkt {
                Packet::ReceiverReport(_) => {}

                // A SourceDescription containing a CNAME must be included in every
                // CompoundPacket.
                Packet::SourceDescription(p) => {
                    for c in &p.chunks {
                        for it in &c.items {
                            if it.sdes_type == SDESType::SDESCNAME {
                                return Ok(it.text.clone());
                            }
                        }
                    }
                }

                _ => return Err(ERR_PACKET_BEFORE_CNAME.clone()),
            };
        }
        Err(ERR_MISSING_CNAME.clone())
    }

    // Marshal encodes the CompoundPacket as binary.
    pub fn marshal<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        self.validate()?;

        // use packet::marshal function
        marshal(&self.0, writer)
    }

    // destination_ssrc returns the synchronization sources associated with this
    // CompoundPacket's reception report.
    pub fn destination_ssrc(&self) -> Vec<u32> {
        if self.0.is_empty() {
            vec![]
        } else {
            match &self.0[0] {
                Packet::SenderReport(p) => p.destination_ssrc(),
                Packet::ReceiverReport(p) => p.destination_ssrc(),
                _ => vec![],
            }
        }
    }
}
