use util::{Buffer, Error};

use tokio::sync::mpsc;

use std::io::Cursor;

// Limit the buffer size to 1MB
pub const SRTP_BUFFER_SIZE: usize = 1000 * 1000;

// Limit the buffer size to 100KB
pub const SRTCP_BUFFER_SIZE: usize = 100 * 1000;

// Stream handles decryption for a single RTP/RTCP SSRC
pub struct Stream {
    ssrc: u32,
    tx: mpsc::Sender<u32>,
    buffer: Buffer,
}

impl Stream {
    pub fn new(ssrc: u32, tx: mpsc::Sender<u32>, buffer_size: usize) -> Self {
        Stream {
            ssrc,
            tx,
            // Create a buffer with a 1MB limit
            buffer: Buffer::new(0, buffer_size),
        }
    }

    // Get Cloned Buffer
    pub(crate) fn get_cloned_buffer(&self) -> Buffer {
        self.buffer.clone()
    }

    // GetSSRC returns the SSRC we are demuxing for
    pub fn get_ssrc(&mut self) -> u32 {
        self.ssrc
    }

    // Read reads and decrypts full RTP packet from the nextConn
    pub async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        self.buffer.read(buf).await
    }

    // ReadRTP reads and decrypts full RTP packet and its header from the nextConn
    pub async fn read_rtp(
        &mut self,
        buf: &mut [u8],
    ) -> Result<(usize, rtp::header::Header), Error> {
        let n = self.buffer.read(buf).await?;
        let mut reader = Cursor::new(buf);
        let header = rtp::header::Header::unmarshal(&mut reader)?;

        Ok((n, header))
    }

    // ReadRTCP reads and decrypts full RTP packet and its header from the nextConn
    pub async fn read_rtcp(
        &mut self,
        buf: &mut [u8],
    ) -> Result<(usize, rtcp::header::Header), Error> {
        let n = self.buffer.read(buf).await?;
        let mut reader = Cursor::new(buf);
        let header = rtcp::header::Header::unmarshal(&mut reader)?;

        Ok((n, header))
    }

    // Close removes the ReadStream from the session and cleans up any associated state
    pub async fn close(&mut self) -> Result<(), Error> {
        self.buffer.close().await;
        self.tx.send(self.ssrc).await?;

        Ok(())
    }
}
