#[cfg(test)]
mod packetizer_test;

use std::fmt;
use std::sync::Arc;
use std::time::SystemTime;

use bytes::{Bytes, BytesMut};
use util::marshal::{Marshal, MarshalSize};

use crate::error::Result;
use crate::extension::abs_send_time_extension::*;
use crate::header::*;
use crate::packet::*;
use crate::sequence::*;

/// Payloader payloads a byte array for use as rtp.Packet payloads
pub trait Payloader: fmt::Debug {
    fn payload(&mut self, mtu: usize, b: &Bytes) -> Result<Vec<Bytes>>;
    fn clone_to(&self) -> Box<dyn Payloader + Send + Sync>;
}

impl Clone for Box<dyn Payloader + Send + Sync> {
    fn clone(&self) -> Box<dyn Payloader + Send + Sync> {
        self.clone_to()
    }
}

/// Packetizer packetizes a payload
pub trait Packetizer: fmt::Debug {
    fn enable_abs_send_time(&mut self, value: u8);
    fn packetize(&mut self, payload: &Bytes, samples: u32) -> Result<Vec<Packet>>;
    fn skip_samples(&mut self, skipped_samples: u32);
    fn clone_to(&self) -> Box<dyn Packetizer + Send + Sync>;
}

impl Clone for Box<dyn Packetizer + Send + Sync> {
    fn clone(&self) -> Box<dyn Packetizer + Send + Sync> {
        self.clone_to()
    }
}

/// Depacketizer depacketizes a RTP payload, removing any RTP specific data from the payload
pub trait Depacketizer {
    fn depacketize(&mut self, b: &Bytes) -> Result<Bytes>;

    /// Checks if the packet is at the beginning of a partition.  This
    /// should return false if the result could not be determined, in
    /// which case the caller will detect timestamp discontinuities.
    fn is_partition_head(&self, payload: &Bytes) -> bool;

    /// Checks if the packet is at the end of a partition.  This should
    /// return false if the result could not be determined.
    fn is_partition_tail(&self, marker: bool, payload: &Bytes) -> bool;
}

//TODO: SystemTime vs Instant?
// non-monotonic clock vs monotonically non-decreasing clock
/// FnTimeGen provides current SystemTime
pub type FnTimeGen = Arc<dyn (Fn() -> SystemTime) + Send + Sync>;

#[derive(Clone)]
pub(crate) struct PacketizerImpl {
    pub(crate) mtu: usize,
    pub(crate) payload_type: u8,
    pub(crate) ssrc: u32,
    pub(crate) payloader: Box<dyn Payloader + Send + Sync>,
    pub(crate) sequencer: Box<dyn Sequencer + Send + Sync>,
    pub(crate) timestamp: u32,
    pub(crate) clock_rate: u32,
    pub(crate) abs_send_time: u8, //http://www.webrtc.org/experiments/rtp-hdrext/abs-send-time
    pub(crate) time_gen: Option<FnTimeGen>,
}

impl fmt::Debug for PacketizerImpl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PacketizerImpl")
            .field("mtu", &self.mtu)
            .field("payload_type", &self.payload_type)
            .field("ssrc", &self.ssrc)
            .field("timestamp", &self.timestamp)
            .field("clock_rate", &self.clock_rate)
            .field("abs_send_time", &self.abs_send_time)
            .finish()
    }
}

pub fn new_packetizer(
    mtu: usize,
    payload_type: u8,
    ssrc: u32,
    payloader: Box<dyn Payloader + Send + Sync>,
    sequencer: Box<dyn Sequencer + Send + Sync>,
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

impl Packetizer for PacketizerImpl {
    fn enable_abs_send_time(&mut self, value: u8) {
        self.abs_send_time = value
    }

    fn packetize(&mut self, payload: &Bytes, samples: u32) -> Result<Vec<Packet>> {
        let payloads = self.payloader.payload(self.mtu - 12, payload)?;
        let payloads_len = payloads.len();
        let mut packets = Vec::with_capacity(payloads_len);
        for (i, payload) in payloads.into_iter().enumerate() {
            packets.push(Packet {
                header: Header {
                    version: 2,
                    padding: false,
                    extension: false,
                    marker: i == payloads_len - 1,
                    payload_type: self.payload_type,
                    sequence_number: self.sequencer.next_sequence_number(),
                    timestamp: self.timestamp, //TODO: Figure out how to do timestamps
                    ssrc: self.ssrc,
                    ..Default::default()
                },
                payload,
            });
        }

        self.timestamp = self.timestamp.wrapping_add(samples);

        if payloads_len != 0 && self.abs_send_time != 0 {
            let st = if let Some(fn_time_gen) = &self.time_gen {
                fn_time_gen()
            } else {
                SystemTime::now()
            };
            let send_time = AbsSendTimeExtension::new(st);
            //apply http://www.webrtc.org/experiments/rtp-hdrext/abs-send-time
            let mut raw = BytesMut::with_capacity(send_time.marshal_size());
            raw.resize(send_time.marshal_size(), 0);
            let _ = send_time.marshal_to(&mut raw)?;
            packets[payloads_len - 1]
                .header
                .set_extension(self.abs_send_time, raw.freeze())?;
        }

        Ok(packets)
    }

    /// skip_samples causes a gap in sample count between Packetize requests so the
    /// RTP payloads produced have a gap in timestamps
    fn skip_samples(&mut self, skipped_samples: u32) {
        self.timestamp = self.timestamp.wrapping_add(skipped_samples);
    }

    fn clone_to(&self) -> Box<dyn Packetizer + Send + Sync> {
        Box::new(self.clone())
    }
}
