use crate::errors::RTPError;
use crate::packet::Packet;
use crate::sequence::Sequencer;
use crate::{extension::abs_send_time_extension::*, header::Header};

use bytes::BytesMut;
use std::time::Duration;

mod packetizer_test;

// Payloader payloads a byte array for use as rtp.Packet payloads
pub trait Payloader {
    fn payload(&self, mtu: u16, payload: &[u8]) -> Vec<Vec<u8>>;
}

/// Packetizer packetizes a payload
pub trait PacketizerInterface {
    fn packetize(&mut self, payload: &mut [u8], samples: u32) -> Result<Vec<Packet>, RTPError>;
    fn enable_abs_send_time(&mut self, value: u8);
}

/// Depacketizer depacketizes a RTP payload, removing any RTP specific data from the payload
pub trait Depacketizer {
    fn unmarshal(&mut self, packet: &mut BytesMut) -> Result<BytesMut, RTPError>;
}

pub type FnTimeGen = fn() -> Duration;

struct Packetizer {
    pub mtu: u16,
    pub payload_type: u8,
    pub ssrc: u32,
    pub payloader: Box<dyn Payloader>,
    pub sequencer: Box<dyn Sequencer>,
    pub timestamp: u32,
    pub clock_rate: u32,
    pub abs_send_time: u8, //http://www.webrtc.org/experiments/rtp-hdrext/abs-send-time
    pub time_gen: Option<FnTimeGen>,
}

pub fn new_packetizer(
    mtu: u16,
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

    fn packetize(&mut self, payload: &mut [u8], samples: u32) -> Result<Vec<Packet>, RTPError> {
        // Guard against an empty payload
        if payload.is_empty() {
            return Ok(vec![]);
        }

        let payloads = self.payloader.payload(self.mtu - 12, payload);
        let mut packets = vec![Packet::default(); payloads.len()];

        for (i, pp) in payloads.iter().enumerate() {
            packets[i].header = Header {
                version: 2,
                marker: i == payloads.len() - 1,
                payload_type: self.payload_type,
                sequence_number: self.sequencer.next_sequence_number(),
                timestamp: self.timestamp,
                ssrc: self.ssrc,
                ..Default::default()
            };

            packets[i].payload = pp.to_vec();
        }

        self.timestamp += samples;

        if !packets.is_empty() && self.abs_send_time != 0 {
            let send_time =
                AbsSendTimeExtension::new(self.time_gen.map_or(Duration::default(), |v| v()));

            // apply http://www.webrtc.org/experiments/rtp-hdrext/abs-send-time
            let b = match send_time.marshal() {
                Ok(e) => e,
                Err(e) => return Err(RTPError::ExtensionError(e)),
            };

            let len = packets.len() - 1;

            if let Err(e) = packets[len].header.set_extension(self.abs_send_time, &b) {
                return Err(e);
            }
        }

        Ok(packets)
    }
}
