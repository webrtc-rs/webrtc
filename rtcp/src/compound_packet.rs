use std::fmt;
use std::io::{Read, Write};

use util::Error;

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use super::errors::*;
use super::header::*;
use super::packet::*;
use super::source_description::SDESType;
use crate::get_padding;
use crate::source_description::SourceDescription;

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
/*
pub struct CompoundPacket<W: Write>(Vec<Box<dyn Packet<W>>>);

impl<W: Write> CompoundPacket<W> {
    /*
    // Validate returns an error if this is not an RFC-compliant CompoundPacket.
    pub fn validate(&self) -> Result<(), Error> {
        if self.len() == 0 {
            return Err(ErrEmptyCompound.clone());
        }

        // SenderReport and ReceiverReport are the only types that
        // are allowed to be the first packet in a compound datagram
        match self[0].header().packet_type {
            PacketType::TypeSenderReport | PacketType::TypeReceiverReport => {}
            _ => return Err(ErrBadFirstPacket.clone()),
        };

        for pkt in &self[1..] {
            match pkt.header().packet_type {
                // If the number of RecetpionReports exceeds 31 additional ReceiverReports
                // can be included here.
                PacketType::TypeReceiverReport => {}

                // A SourceDescription containing a CNAME must be included in every
                // CompoundPacket.
                PacketType::TypeSourceDescription => {
                    let mut has_cname = false;
                    let pkt = pkt as SourceDescription;
                    for c in &pkt.chunks {
                        for it in &c.items {
                            if it.sdes_type == SDESType::SDESCNAME {
                                has_cname = true
                            }
                        }
                    }

                    if !has_cname {
                        return Err(ErrMissingCNAME.clone());
                    }

                    return Ok(());
                }
                // Other packets are not permitted before the CNAME
                _ => return Err(ErrPacketBeforeCNAME.clone()),
            };
        }

        // CNAME never reached
        Err(ErrMissingCNAME.clone())
    }

    //CNAME returns the CNAME that *must* be present in every CompoundPacket
    pub fn cname(&self) -> Result<String, Error> {
        if self.len() < 1 {
            return Err(ErrEmptyCompound.clone());
        }

        for pkt in &self[1..] {
            sdes, ok := pkt.(*SourceDescription)
            if ok {
                for _, c := range sdes.Chunks {
                    for _, it := range c.Items {
                        if it.Type == SDESCNAME {
                            return it.Text, err
                        }
                    }
                }
            } else {
                _, ok := pkt.(*ReceiverReport)
                if !ok {
                    err = errPacketBeforeCNAME
                }
            }
        }
        return "", errMissingCNAME
    }

    // Marshal encodes the CompoundPacket as binary.
    func (c CompoundPacket) Marshal() ([]byte, error) {
        if err := c.Validate(); err != nil {
            return nil, err
        }

        p := []Packet(c)
        return Marshal(p)
    }

    // Unmarshal decodes a CompoundPacket from binary.
    func (c *CompoundPacket) Unmarshal(rawData []byte) error {
        out := make(CompoundPacket, 0)
        for len(rawData) != 0 {
            p, processed, err := unmarshal(rawData)

            if err != nil {
                return err
            }

            out = append(out, p)
            rawData = rawData[processed:]
        }
        *c = out

        if err := c.Validate(); err != nil {
            return err
        }

        return nil
    }
    */
    // DestinationSSRC returns the synchronization sources associated with this
    // CompoundPacket's reception report.
    pub fn destination_ssrc(&self) -> Vec<u32> {
        if self.0.len() == 0 {
            vec![]
        } else {
            self.0[0].destination_ssrc()
        }
    }
}
*/
