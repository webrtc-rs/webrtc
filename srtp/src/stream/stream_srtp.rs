use super::*;

use rtp::packet::Header;
use util::buffer::*;
use util::{Buffer, Error};

use std::io::Cursor;

use tokio::sync::Lock;

// Limit the buffer size to 1MB
const SRTP_BUFFER_SIZE: usize = 1000 * 1000;

// ReadStreamSRTP handles decryption for a single RTP SSRC
pub struct ReadStreamSRTP {
    is_inited: Lock<bool>,
    is_closed: bool,

    //session: Option<SessionSRTP>,
    ssrc: u32,

    buffer: Buffer,
}

#[async_trait]
impl ReadStream for ReadStreamSRTP {
    async fn init(&mut self, ssrc: u32) -> Result<(), Error> {
        let mut is_inited = self.is_inited.lock().await;

        if *is_inited {
            return Err(Error::new(
                "ReadStreamSRTP has already been inited".to_string(),
            ));
        }

        self.ssrc = ssrc;
        *is_inited = true;

        Ok(())
    }

    // Read reads and decrypts full RTP packet from the nextConn
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        self.buffer.read(buf).await
    }

    // GetSSRC returns the SSRC we are demuxing for
    fn get_ssrc(&mut self) -> u32 {
        self.ssrc
    }
}

impl ReadStreamSRTP {
    pub(crate) fn new() -> Self {
        ReadStreamSRTP {
            is_inited: Lock::new(false),
            is_closed: false,

            //session: None,
            ssrc: 0,

            // Create a buffer with a 1MB limit
            buffer: Buffer::new(0, SRTP_BUFFER_SIZE),
        }
    }

    pub(crate) async fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        let result = self.buffer.write(buf).await;
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

    // ReadRTP reads and decrypts full RTP packet and its header from the nextConn
    pub(crate) async fn read_rtp(&mut self, buf: &mut [u8]) -> Result<(usize, Header), Error> {
        let n = self.buffer.read(buf).await?;
        let mut reader = Cursor::new(buf);
        let header = Header::unmarshal(&mut reader)?;

        Ok((n, header))
    }

    // Close removes the ReadStream from the session and cleans up any associated state
    pub(crate) async fn close(&mut self) -> Result<(), Error> {
        let is_inited = self.is_inited.lock().await;

        if !(*is_inited) {
            return Err(Error::new("ReadStreamSRTP has not been inited".to_string()));
        }

        if self.is_closed {
            return Err(Error::new("ReadStreamSRTP is already closed".to_string()));
        }

        self.is_closed = true;

        self.buffer.close().await;

        /*let ssrc = r.ssrc;
        if let Some(session) = &mut r.session {
            session.session.remove_read_stream(ssrc);
        } else {
            return Err(Error::new("ReadStreamSRTP has empty session".to_string()));
        }*/

        Ok(())
    }
}

/*
// WriteStreamSRTP is stream for a single Session that is used to encrypt RTP
pub struct WriteStreamSRTP<'a>  {
    session: Option<&'a SessionSRTP<'a>>,
}

impl<'a> WriteStreamSRTP<'a> {
    pub fn new(session: &SessionSRTP<'a>) -> Self{
        WriteStreamSRTP{
            session: Some(session),
        }
    }
    /*
    // WriteRTP encrypts a RTP packet and writes to the connection
    pub fn write_rtp(&mut self, header: &Header, payload:&[8]) ->Result<usize, Error> {
        .session.writeRTP(header, payload)
    }

    // Write encrypts and writes a full RTP packets to the nextConn
    func (w * WriteStreamSRTP) Write(b []byte) (int, error) {
    return w.session.write(b)
    }
    */
}
*/
