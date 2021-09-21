use anyhow::Result;
use std::time::{Duration, SystemTime};

pub mod ivf_reader;
pub mod ivf_writer;
pub mod ogg_reader;
pub mod ogg_writer;

/// A Sample contains encoded media and timing information
pub struct Sample {
    pub data: Vec<u8>,
    pub timestamp: SystemTime,
    pub duration: Duration,
    pub packet_timestamp: u32,
    pub prev_dropped_packets: u16,
}

// Writer defines an interface to handle
// the creation of media files
pub trait Writer {
    // Add the content of an RTP packet to the media
    fn write_rtp(pkt: &rtp::packet::Packet) -> Result<()>;
    // close the media
    // Note: close implementation must be idempotent
    fn close() -> Result<()>;
}
