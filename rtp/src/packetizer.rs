use crate::header::*;
use crate::packet::*;
use crate::sequence::*;

use std::io::Read;
use std::time::{Duration, SystemTime};

use util::Error;

#[cfg(test)]
mod packetizer_test;

// Payloader payloads a byte array for use as rtp.Packet payloads
pub trait Payloader {
    fn payload<R: Read>(&self, mtu: isize, reader: &mut R) -> Result<Vec<Vec<u8>>, Error>;
}

// Packetizer packetizes a payload
pub trait Packetizer {
    fn packetize<R: Read, P: Payloader, S: Sequencer>(
        &mut self,
        reader: &mut R,
        payloader: &mut P,
        sequencer: &mut S,
        samples: u32,
    ) -> Result<Vec<Packet>, Error>;
    fn enable_abs_send_time(&mut self, value: isize);
}

// Depacketizer depacketizes a RTP payload, removing any RTP specific data from the payload
pub trait Depacketizer {
    fn depacketize<R: Read>(&mut self, reader: &mut R) -> Result<(), Error>;
}

struct PacketizerImpl {
    mtu: isize,
    payload_type: u8,
    ssrc: u32,
    timestamp: u32,
    clock_rate: u32,
    abs_send_time: isize, //http://www.webrtc.org/experiments/rtp-hdrext/abs-send-time
}

impl PacketizerImpl {
    pub fn new(mtu: isize, payload_type: u8, ssrc: u32, clock_rate: u32) -> Self {
        PacketizerImpl {
            mtu,
            payload_type,
            ssrc,
            timestamp: rand::random::<u32>(), //TODO: globalMathRandomGenerator?
            clock_rate,
            abs_send_time: 0,
        }
    }
}

impl Packetizer for PacketizerImpl {
    fn enable_abs_send_time(&mut self, value: isize) {
        self.abs_send_time = value
    }

    fn packetize<R: Read, P: Payloader, S: Sequencer>(
        &mut self,
        reader: &mut R,
        payloader: &mut P,
        sequencer: &mut S,
        samples: u32,
    ) -> Result<Vec<Packet>, Error> {
        let payloads = payloader.payload(self.mtu - 12, reader)?;
        let mut packets = vec![];
        let (mut i, l) = (0, payloads.len());
        for payload in payloads {
            packets.push(Packet {
                header: Header {
                    version: 2,
                    padding: false,
                    extension: false,
                    marker: i == l - 1,
                    payload_type: self.payload_type,
                    sequence_number: sequencer.next_sequence_number(),
                    timestamp: self.timestamp, //TODO: Figure out how to do timestamps
                    ssrc: self.ssrc,
                    ..Default::default()
                },
                payload,
            });
            i += 1;
        }

        self.timestamp += samples;

        if l != 0 && self.abs_send_time != 0 {
            let d = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?;
            let t = unix2ntp(d) >> 14;
            //apply http://www.webrtc.org/experiments/rtp-hdrext/abs-send-time
            //packets[l - 1].header.extension = true;
            //packets[l - 1].header.extension_profile = 0xBEDE;
            /*packets[l - 1].header.extension_payload = vec![
                //the first byte is
                // 0 1 2 3 4 5 6 7
                //+-+-+-+-+-+-+-+-+
                //|  ID   |  len  |
                //+-+-+-+-+-+-+-+-+
                //per RFC 5285
                //Len is the number of bytes in the extension - 1
                ((self.abs_send_time << 4) | 2) as u8,
                (t & 0xFF0000 >> 16) as u8,
                (t & 0xFF00 >> 8) as u8,
                (t & 0xFF) as u8,
            ];*/
        }

        Ok(packets)
    }
}

fn unix2ntp(t: Duration) -> u64 {
    let u = t.as_nanos() as u64;
    let mut s = u / 1000000000;
    s += 0x83AA7E80; //offset in seconds between unix epoch and ntp epoch
    let mut f = u % 1000000000;
    f <<= 32;
    f /= 1000000000;
    s <<= 32;

    return s | f;
}
