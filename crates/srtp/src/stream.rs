use crate::error::Error;
use rtp::packetizer::Marshaller;
use util::Buffer;

use bytes::{Bytes, BytesMut};
use tokio::sync::mpsc;

/// Limit the buffer size to 1MB
pub const SRTP_BUFFER_SIZE: usize = 1000 * 1000;

/// Limit the buffer size to 100KB
pub const SRTCP_BUFFER_SIZE: usize = 100 * 1000;

/// Stream handles decryption for a single RTP/RTCP SSRC
#[derive(Debug)]
pub struct Stream {
    ssrc: u32,
    tx: mpsc::Sender<u32>,
    buffer: Buffer,
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

    /// Get Cloned Buffer
    pub(crate) fn get_cloned_buffer(&self) -> Buffer {
        self.buffer.clone()
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
    pub async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        Ok(self.buffer.read(buf, None).await?)
    }

    /// ReadRTP reads and decrypts full RTP packet and its header from the nextConn
    pub async fn read_rtp(
        &mut self,
        buf: &mut BytesMut,
    ) -> Result<(usize, rtp::header::Header), Error> {
        if !self.is_rtp {
            return Err(Error::InvalidRtpStream);
        }

        let n = self.buffer.read(buf, None).await?;
        let b = Bytes::from(buf[..std::cmp::min(rtp::header::HEADER_LENGTH, n)].to_vec());
        let header = rtp::header::Header::unmarshal(&b)?;

        Ok((n, header))
    }

    /// ReadRTCP reads and decrypts full RTP packet and its header from the nextConn
    pub async fn read_rtcp(
        &mut self,
        buf: &mut BytesMut,
    ) -> Result<(usize, rtcp::header::Header), Error> {
        if self.is_rtp {
            return Err(Error::InvalidRtcpStream);
        }

        let n = self.buffer.read(buf, None).await?;
        let b = Bytes::from(buf[..std::cmp::min(rtcp::header::HEADER_LENGTH, n)].to_vec());
        let header = rtcp::header::Header::unmarshal(&b)?;

        Ok((n, header))
    }

    /// Close removes the ReadStream from the session and cleans up any associated state
    pub async fn close(&mut self) -> Result<(), Error> {
        self.buffer.close().await;
        self.tx.send(self.ssrc).await?;

        Ok(())
    }
}
