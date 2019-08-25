use std::io::{Read, Write};

use utils::Error;

// Packet represents an RTCP packet, a protocol used for out-of-band statistics and control information for an RTP session
/*
pub trait Packet<R: Read, W: Write> {
    // DestinationSSRC returns an array of SSRC values that this packet refers to.
    fn destination_ssrc(&self) -> Vec<u32>;
    fn marshal(&self, writer: &mut W) -> Result<(), Error>;
    fn unmarshal(reader: &mut R) -> Result<Box<dyn Packet<R, W>>, Error>;
}

// Unmarshal takes an entire udp datagram (which may consist of multiple RTCP packets) and
// returns the unmarshaled packets it contains.
//
// If this is a reduced-size RTCP packet a feedback packet (Goodbye, SliceLossIndication, etc)
// will be returned. Otherwise, the underlying type of the returned packet will be
// CompoundPacket.
pub fn unmarshal<R: Read>(reader: &mut R) -> Result<Vec<impl Packet<R, W>>, Error> {
    let mut packets = vec![];
    /*
        p, processed, err := unmarshal(rawData)

        if err != nil {
            return nil, err
        }

        packets = append(packets, p)
        rawData = rawData[processed:]

    switch len(packets) {
    // Empty packet
    case 0:
        return nil, errInvalidHeader
    // Multiple Packets
    default:
        return packets, nil
    }*/
    Ok(packets)
}

//Marshal takes an array of Packets and serializes them to a single buffer
pub fn marshal<W: Write>(packets: &[impl Packet<R, W>], writer: &mut W) -> Result<(), Error> {
    for p in packets {
        p.marshal(writer)?;
    }
    Ok(())
}
*/
