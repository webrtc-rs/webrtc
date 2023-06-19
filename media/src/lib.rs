#![warn(rust_2018_idioms)]
#![allow(dead_code)]

pub mod audio;
mod error;
pub mod io;
pub mod video;

use std::time::{Duration, SystemTime};

use bytes::Bytes;
pub use error::Error;

/// A Sample contains encoded media and timing information
#[derive(Debug)]
pub struct Sample {
    /// The assembled data in the sample, as a bitstream.
    ///
    /// The format is Codec dependant, but is always a bitstream format
    /// rather than the packetized format used when carried over RTP.
    ///
    /// See: [`rtp::packetizer::Depacketizer`] and implementations of it for more details.
    pub data: Bytes,

    /// The wallclock time when this sample was generated.
    pub timestamp: SystemTime,

    /// The duration of this sample
    pub duration: Duration,

    /// The RTP packet timestamp of this sample.
    ///
    /// For all RTP packets that contributed to a single sample the timestamp is the same.
    pub packet_timestamp: u32,

    /// The number of packets that were dropped prior to building this sample.
    ///
    /// Packets being dropped doesn't necessarily indicate something wrong, e.g., packets are sometimes
    /// dropped because they aren't relevant for sample building.
    pub prev_dropped_packets: u16,

    /// The number of packets that were identified as padding prior to building this sample.
    ///
    /// Some implementations, notably libWebRTC, send padding packets to keep the send rate steady.
    /// These packets don't carry media and aren't useful for building samples.
    ///
    /// This field can be combined with [`Sample::prev_dropped_packets`] to determine if any
    /// dropped packets are likely to have detrimental impact on the steadiness of the RTP stream.
    ///
    /// ## Example adjustment
    ///
    /// ```rust
    /// # use bytes::Bytes;
    /// # use std::time::{SystemTime, Duration};
    /// # use webrtc_media::Sample;
    /// # let sample = Sample {
    /// #   data: Bytes::new(),
    /// #   timestamp: SystemTime::now(),
    /// #   duration: Duration::from_secs(0),
    /// #   packet_timestamp: 0,
    /// #   prev_dropped_packets: 10,
    /// #   prev_padding_packets: 15
    /// # };
    /// #
    /// let adjusted_dropped =
    /// sample.prev_dropped_packets.saturating_sub(sample.prev_padding_packets);
    /// ```
    pub prev_padding_packets: u16,
}

impl Default for Sample {
    fn default() -> Self {
        Sample {
            data: Bytes::new(),
            timestamp: SystemTime::now(),
            duration: Duration::from_secs(0),
            packet_timestamp: 0,
            prev_dropped_packets: 0,
            prev_padding_packets: 0,
        }
    }
}

impl PartialEq for Sample {
    fn eq(&self, other: &Self) -> bool {
        let mut equal: bool = true;
        if self.data != other.data {
            equal = false;
        }
        if self.timestamp.elapsed().unwrap().as_secs()
            != other.timestamp.elapsed().unwrap().as_secs()
        {
            equal = false;
        }
        if self.duration != other.duration {
            equal = false;
        }
        if self.packet_timestamp != other.packet_timestamp {
            equal = false;
        }
        if self.prev_dropped_packets != other.prev_dropped_packets {
            equal = false;
        }
        if self.prev_padding_packets != other.prev_padding_packets {
            equal = false;
        }

        equal
    }
}
