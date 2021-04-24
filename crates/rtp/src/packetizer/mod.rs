#[cfg(test)]
mod packetizer_test;

use crate::{
    error::Error, extension::abs_send_time_extension::*, header::*, packet::*, sequence::*,
};

use bytes::{Bytes, BytesMut};
use std::marker::Sized;
use std::time::{Duration, SystemTime};

pub trait Marshaller {
    fn unmarshal(raw_packet: &Bytes) -> Result<Self, Error>
    where
        Self: Sized;
    fn marshal_size(&self) -> usize;
    fn marshal_to(&self, buf: &mut BytesMut) -> Result<usize, Error>;
    fn marshal(&self) -> Result<Bytes, Error> {
        let mut buf = BytesMut::with_capacity(self.marshal_size());
        let _ = self.marshal_to(&mut buf)?;
        Ok(buf.freeze())
    }
}

/// Payloader payloads a byte array for use as rtp.Packet payloads
pub trait Payloader {
    fn payload(&self, mtu: usize, b: &Bytes) -> Result<Vec<Bytes>, Error>;
}

/// Packetizer packetizes a payload
pub trait Packetizer {
    fn enable_abs_send_time(&mut self, value: u8);
    fn packetize(&mut self, payload: &Bytes, samples: u32) -> Result<Vec<Packet>, Error>;
}

/// Depacketizer depacketizes a RTP payload, removing any RTP specific data from the payload
pub trait Depacketizer {
    fn depacketize(&mut self, b: &Bytes) -> Result<(), Error>;
}

pub type FnTimeGen = fn() -> Duration;

pub(crate) struct PacketizerImpl<P: Payloader, S: Sequencer> {
    pub(crate) mtu: usize,
    pub(crate) payload_type: u8,
    pub(crate) ssrc: u32,
    pub(crate) payloader: P,
    pub(crate) sequencer: S,
    pub(crate) timestamp: u32,
    pub(crate) clock_rate: u32,
    pub(crate) abs_send_time: u8, //http://www.webrtc.org/experiments/rtp-hdrext/abs-send-time
    pub(crate) time_gen: Option<FnTimeGen>,
}

pub fn new_packetizer<P: Payloader, S: Sequencer>(
    mtu: usize,
    payload_type: u8,
    ssrc: u32,
    payloader: P,
    sequencer: S,
    clock_rate: u32,
) -> impl Packetizer {
    PacketizerImpl {
        mtu,
        payload_type,
        ssrc,
        payloader,
        sequencer,
        timestamp: rand::random::<u32>(),
        clock_rate,
        abs_send_time: 0,
        time_gen: None,
    }
}

impl<P: Payloader, S: Sequencer> Packetizer for PacketizerImpl<P, S> {
    fn enable_abs_send_time(&mut self, value: u8) {
        self.abs_send_time = value
    }

    fn packetize(&mut self, payload: &Bytes, samples: u32) -> Result<Vec<Packet>, Error> {
        let payloads = self.payloader.payload(self.mtu - 12, payload)?;
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
                    sequence_number: self.sequencer.next_sequence_number(),
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
            let d = if let Some(fn_time_gen) = &self.time_gen {
                fn_time_gen()
            } else {
                SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?
            };
            let send_time = AbsSendTimeExtension::new(d);
            //apply http://www.webrtc.org/experiments/rtp-hdrext/abs-send-time
            let mut raw = BytesMut::with_capacity(send_time.marshal_size());
            let _ = send_time.marshal_to(&mut raw)?;
            packets[l - 1]
                .header
                .set_extension(self.abs_send_time, raw.freeze())?;
        }

        Ok(packets)
    }
}
