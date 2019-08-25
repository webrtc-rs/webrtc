use std::io::{Read, Write};

use utils::Error;

use super::header::*;
use super::raw_packet::*;

// Packet represents an RTCP packet, a protocol used for out-of-band statistics and control information for an RTP session

pub trait Packet<W: Write> {
    // DestinationSSRC returns an array of SSRC values that this packet refers to.
    fn destination_ssrc() -> Vec<u32>;
    fn marshal(&self, writer: &mut W) -> Result<(), Error>;
}

//Marshal takes an array of Packets and serializes them to a single buffer
pub fn marshal<W: Write>(packets: &[impl Packet<W>], writer: &mut W) -> Result<(), Error> {
    for p in packets {
        p.marshal(writer)?;
    }
    Ok(())
}

// Unmarshal takes an entire udp datagram (which may consist of multiple RTCP packets) and
// returns the unmarshaled packets it contains.
//
// If this is a reduced-size RTCP packet a feedback packet (Goodbye, SliceLossIndication, etc)
// will be returned. Otherwise, the underlying type of the returned packet will be
// CompoundPacket.
/*
pub fn unmarshal<R: Read, W: Write>(reader: &mut R) -> Result<Vec<impl Packet<R, W>>, Error> {
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
    // Ok(packets)
    Err(Error::new("unimplemented".to_string()))
}*/

// unmarshal is a factory which pulls the first RTCP packet from a bytestream,
// and returns it's parsed representation, and the amount of data that was processed.
fn _unmarshal<R: Read, W: Write>(reader: &mut R) -> Result<impl Packet<W>, Error> {
    let header = Header::unmarshal(reader)?;

    let mut in_packet = reader.take(header.length as u64 * 4);

    let packet = match header.packet_type {
        /*case TypeSenderReport:
                packet = new(SenderReport)

            case TypeReceiverReport:
                packet = new(ReceiverReport)

            case TypeSourceDescription:
                packet = new(SourceDescription)

            case TypeGoodbye:
                packet = new(Goodbye)

            case TypeTransportSpecificFeedback:
                switch h.Count {
                case FormatTLN:
                    packet = new(TransportLayerNack)
                case FormatRRR:
                    packet = new(RapidResynchronizationRequest)
                default:
                    packet = new(RawPacket)
                }

            case TypePayloadSpecificFeedback:
                switch h.Count {
                case FormatPLI:
                    packet = new(PictureLossIndication)
                case FormatSLI:
                    packet = new(SliceLossIndication)
                case FormatREMB:
                    packet = new(ReceiverEstimatedMaximumBitrate)
                default:
                    packet = new(RawPacket)
                }
        */
        _ => RawPacket::unmarshal(&mut in_packet)?,
    };

    Ok(packet)
}
