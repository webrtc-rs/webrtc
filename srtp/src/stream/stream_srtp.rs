use rtp::packet::Header;
use util::buffer::*;
use util::{Buffer, Error};

use tokio::sync::mpsc;

use std::io::Cursor;

// Limit the buffer size to 1MB
const SRTP_BUFFER_SIZE: usize = 1000 * 1000;

// StreamSRTP handles decryption for a single RTP SSRC
pub struct StreamSRTP {
    ssrc: u32,
    tx: mpsc::Sender<u32>,
    buffer: Buffer,
}

impl StreamSRTP {
    pub fn new(ssrc: u32, tx: mpsc::Sender<u32>) -> Self {
        StreamSRTP {
            ssrc,
            tx,
            // Create a buffer with a 1MB limit
            buffer: Buffer::new(0, SRTP_BUFFER_SIZE),
        }
    }

    // Get Cloned Buffer
    pub(crate) fn get_cloned_buffer(&self) -> Buffer {
        self.buffer.clone()
    }

    pub(crate) async fn write(buffer: &mut Buffer, buf: &[u8]) -> Result<usize, Error> {
        let result = buffer.write(buf).await;
        match result {
            Ok(size) => Ok(size),
            Err(err) => {
                if err == ERR_BUFFER_FULL.clone() {
                    // Silently drop data when the buffer is full.
                    Ok(buf.len())
                } else {
                    Err(err)
                }
            }
        }
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
    pub async fn read_rtp(&mut self, buf: &mut [u8]) -> Result<(usize, Header), Error> {
        let n = self.buffer.read(buf).await?;
        let mut reader = Cursor::new(buf);
        let header = Header::unmarshal(&mut reader)?;

        Ok((n, header))
    }

    // Close removes the ReadStream from the session and cleans up any associated state
    pub async fn close(&mut self) -> Result<(), Error> {
        self.buffer.close().await;
        self.tx.send(self.ssrc).await?;

        Ok(())
    }
}
