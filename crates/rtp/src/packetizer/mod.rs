use crate::error::Error;
use crate::extension::abs_send_time_extension::*;
use crate::header::*;
use crate::packet::*;
use crate::sequence::*;

use std::io::{BufWriter, Read};
use std::time::{Duration, SystemTime};

#[cfg(test)]
mod packetizer_test;

// Payloader payloads a byte array for use as rtp.Packet payloads
pub trait Payloader {
    fn payload(&self, mtu: usize, payload: BytesMut) -> Vec<Vec<u8>>;
}

// Packetizer packetizes a payload
pub trait PacketizerInterface {
    fn packetize(&mut self, payload: &mut BytesMut, samples: u32) -> Result<Vec<Packet>, RTPError>;
    fn enable_abs_send_time(&mut self, value: u8);
}

// Depacketizer depacketizes a RTP payload, removing any RTP specific data from the payload
pub trait Depacketizer {
    fn unmarshal(&mut self, packet: &mut BytesMut) -> Result<BytesMut, RTPError>;
}

pub type FnTimeGen = fn() -> Duration;

struct Packetizer {
    mtu: usize,
    payload_type: u8,
    ssrc: u32,
    payloader: Box<dyn Payloader>,
    sequencer: Box<dyn Sequencer>,
    timestamp: u32,
    clock_rate: u32,
    abs_send_time: u8, //http://www.webrtc.org/experiments/rtp-hdrext/abs-send-time
    time_gen: Option<FnTimeGen>,
}

pub fn new_packetizer(
    mtu: usize,
    payload_type: u8,
    ssrc: u32,
    clock_rate: u32,
    payloader: Box<dyn Payloader>,
    sequencer: Box<dyn Sequencer>,
) -> impl PacketizerInterface {
    Packetizer {
        mtu,
        payload_type,
        ssrc,
        sequencer,
        payloader,
        timestamp: rand::random::<u32>(),
        clock_rate,
        abs_send_time: 0,
        time_gen: None,
    }
}

impl PacketizerInterface for Packetizer {
    fn enable_abs_send_time(&mut self, value: u8) {
        self.abs_send_time = value
    }

    fn packetize(&mut self, payload: &mut BytesMut, samples: u32) -> Result<Vec<Packet>, RTPError> {
        // Guard against an empty payload
        if payload.len() == 0 {
            return Ok(vec![]);
        }

        let payloads = self.payloader.payload(self.mtu - 12, payload.clone());
        let mut packets = vec![Packet::default(); payloads.len()];

        for (i, pp) in payloads.iter().enumerate() {
            packets[i].header = Header {
                version: 2,
                marker: 1 == payloads.len() - 1,
                payload_type: self.payload_type,
                sequence_number: self.sequencer.next_sequence_number(),
                timestamp: self.timestamp,
                ssrc: self.ssrc,
                ..Default::default()
            };

            packets[i].payload = pp.to_owned();
        }

        self.timestamp += samples;

        if packets.len() != 0 && self.abs_send_time != 0 {
            let send_time = AbsSendTimeExtension::new(
                self.time_gen.map_or_else(|| Duration::default(), |v| v()),
            );

            // apply http://www.webrtc.org/experiments/rtp-hdrext/abs-send-time
            let b = match send_time.marshal() {
                Ok(e) => e,
                Err(e) => return Err(e),
            };

            let len = packets.len() - 1;

            match packets[len].header.set_extension(self.abs_send_time, &b) {
                Err(e) => return Err(e),

                _ => {}
            }
        }

        return Ok(packets);
    }
}
