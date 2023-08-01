use tokio::sync::mpsc;
use util::marshal::*;
use util::Buffer;

use crate::error::{Error, Result};

/// Limit the buffer size to 1MB
pub const SRTP_BUFFER_SIZE: usize = 1000 * 1000;

/// Limit the buffer size to 100KB
pub const SRTCP_BUFFER_SIZE: usize = 100 * 1000;

/// Stream handles decryption for a single RTP/RTCP SSRC
#[derive(Debug)]
pub struct Stream {
    ssrc: u32,
    tx: mpsc::Sender<u32>,
    pub(crate) buffer: Buffer,
    is_rtp: bool,
}

impl Stream {
    /// Create a new stream
    pub fn new(ssrc: u32, tx: mpsc::Sender<u32>, is_rtp: bool) -> Self {
        Stream {
            ssrc,
            tx,
            // Create a buffer with a 1MB limit
            buffer: Buffer::new(
                0,
                if is_rtp {
                    SRTP_BUFFER_SIZE
                } else {
                    SRTCP_BUFFER_SIZE
                },
            ),
            is_rtp,
        }
    }

    /// GetSSRC returns the SSRC we are demuxing for
    pub fn get_ssrc(&self) -> u32 {
        self.ssrc
    }

    /// Check if RTP is a stream.
    pub fn is_rtp_stream(&self) -> bool {
        self.is_rtp
    }

    /// Read reads and decrypts full RTP packet from the nextConn
    pub async fn read(&self, buf: &mut [u8]) -> Result<usize> {
        Ok(self.buffer.read(buf, None).await?)
    }

    /// ReadRTP reads and decrypts full RTP packet and its header from the nextConn
    pub async fn read_rtp(&self, buf: &mut [u8]) -> Result<rtp::packet::Packet> {
        if !self.is_rtp {
            return Err(Error::InvalidRtpStream);
        }

        let n = self.buffer.read(buf, None).await?;
        let mut b = &buf[..n];
        let pkt = rtp::packet::Packet::unmarshal(&mut b)?;

        Ok(pkt)
    }

    /// read_rtcp reads and decrypts full RTP packet and its header from the nextConn
    pub async fn read_rtcp(
        &self,
        buf: &mut [u8],
    ) -> Result<Vec<Box<dyn rtcp::packet::Packet + Send + Sync>>> {
        if self.is_rtp {
            return Err(Error::InvalidRtcpStream);
        }

        let n = self.buffer.read(buf, None).await?;
        let mut b = &buf[..n];
        let pkt = rtcp::packet::unmarshal(&mut b)?;

        Ok(pkt)
    }

    /// Close removes the ReadStream from the session and cleans up any associated state
    pub async fn close(&self) -> Result<()> {
        self.buffer.close().await;
        let _ = self.tx.send(self.ssrc).await;
        Ok(())
    }
}
