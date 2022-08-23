#![warn(rust_2018_idioms)]
#![allow(dead_code)]

pub mod audio;
mod error;
pub mod io;
pub mod track;
pub mod video;

pub use error::Error;

use bytes::Bytes;
use std::time::{Duration, SystemTime};

/// A Sample contains encoded media and timing information
#[derive(Debug)]
pub struct Sample {
    pub data: Bytes,
    pub timestamp: SystemTime,
    pub duration: Duration,
    pub packet_timestamp: u32,
    pub prev_dropped_packets: u16,
}

impl Default for Sample {
    fn default() -> Self {
        Sample {
            data: Bytes::new(),
            timestamp: SystemTime::now(),
            duration: Duration::from_secs(0),
            packet_timestamp: 0,
            prev_dropped_packets: 0,
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
        equal
    }
}
