#[cfg(test)]
mod packetizer_test;

use crate::error::Result;
use crate::{extension::abs_send_time_extension::*, header::*, packet::*, sequence::*};
use util::marshal::{Marshal, MarshalSize};

use async_trait::async_trait;
use bytes::{Bytes, BytesMut};
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::SystemTime;

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
#[async_trait]
pub trait Packetizer: fmt::Debug {
    fn enable_abs_send_time(&mut self, value: u8);
    async fn packetize(&mut self, payload: &Bytes, samples: u32) -> Result<Vec<Packet>>;
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
pub type FnTimeGen =
    Arc<dyn (Fn() -> Pin<Box<dyn Future<Output = SystemTime> + Send + 'static>>) + Send + Sync>;

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

#[async_trait]
impl Packetizer for PacketizerImpl {
    fn enable_abs_send_time(&mut self, value: u8) {
        self.abs_send_time = value
    }

    async fn packetize(&mut self, payload: &Bytes, samples: u32) -> Result<Vec<Packet>> {
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

        self.timestamp = self.timestamp.wrapping_add(samples);

        if l != 0 && self.abs_send_time != 0 {
            let st = if let Some(fn_time_gen) = &self.time_gen {
                fn_time_gen().await
            } else {
                SystemTime::now()
            };
            let send_time = AbsSendTimeExtension::new(st);
            //apply http://www.webrtc.org/experiments/rtp-hdrext/abs-send-time
            let mut raw = BytesMut::with_capacity(send_time.marshal_size());
            raw.resize(send_time.marshal_size(), 0);
            let _ = send_time.marshal_to(&mut raw)?;
            packets[l - 1]
                .header
                .set_extension(self.abs_send_time, raw.freeze())?;
        }

        Ok(packets)
    }

    /// skip_samples causes a gap in sample count between Packetize requests so the
    /// RTP payloads produced have a gap in timestamps
    fn skip_samples(&mut self, skipped_samples: u32) {
        self.timestamp += skipped_samples;
    }

    fn clone_to(&self) -> Box<dyn Packetizer + Send + Sync> {
        Box::new(self.clone())
    }
}
