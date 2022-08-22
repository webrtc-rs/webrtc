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
