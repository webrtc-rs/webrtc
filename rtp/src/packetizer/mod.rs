use crate::extension::abs_send_time_extension::*;
use crate::header::*;
use crate::packet::*;
use crate::sequence::*;

use util::Error;

use std::io::{BufWriter, Read};
use std::time::{Duration, SystemTime};

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
    fn enable_abs_send_time(&mut self, value: u8);
}

// Depacketizer depacketizes a RTP payload, removing any RTP specific data from the payload
pub trait Depacketizer {
    fn depacketize<R: Read>(&mut self, reader: &mut R) -> Result<(), Error>;
}

pub type FnTimeGen = fn() -> Duration;

struct PacketizerImpl {
    mtu: isize,
    payload_type: u8,
    ssrc: u32,
    timestamp: u32,
    clock_rate: u32,
    abs_send_time: u8, // http://www.webrtc.org/experiments/rtp-hdrext/abs-send-time
    time_gen: Option<FnTimeGen>,
}

impl PacketizerImpl {
    pub fn new(mtu: isize, payload_type: u8, ssrc: u32, clock_rate: u32) -> Self {
        PacketizerImpl {
            mtu,
            payload_type,
            ssrc,
            timestamp: rand::random::<u32>(), // TODO: globalMathRandomGenerator?
            clock_rate,
            abs_send_time: 0,
            time_gen: None,
        }
    }
}

impl Packetizer for PacketizerImpl {
    fn enable_abs_send_time(&mut self, value: u8) {
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
                    timestamp: self.timestamp, // TODO: Figure out how to do timestamps
                    ssrc: self.ssrc,
                    ..Default::default()
                },

                payload,
                ..Default::default()
            });
            i += 1;
        }

        self.timestamp += samples;

        if l != 0 && self.abs_send_time != 0 {
            let d = if let Some(fn_time_gen) = &self.time_gen {
                fn_time_gen()
            } else {
                SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?
            };
            let send_time = AbsSendTimeExtension::new(d);

            // apply http://www.webrtc.org/experiments/rtp-hdrext/abs-send-time
            let mut raw: Vec<u8> = vec![];
            {
                let mut writer = BufWriter::<&mut Vec<u8>>::new(raw.as_mut());
                send_time.marshal(&mut writer)?;
            }

            packets[l - 1]
                .header
                .set_extension(self.abs_send_time, &raw)?;
        }

        Ok(packets)
    }
}
