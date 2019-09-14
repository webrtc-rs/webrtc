use crate::session::session_srtp::SessionSRTP;
use crate::session::Session;

use super::*;

use rtp::packet::Header;
use util::buffer::*;
use util::{Buffer, Error};

use std::io::Cursor;

use tokio::sync::Lock;

// Limit the buffer size to 1MB
const SRTP_BUFFER_SIZE: usize = 1000 * 1000;

// ReadStreamSRTP handles decryption for a single RTP SSRC
struct ReadStreamSRTPInternal {
    is_inited: bool,
    is_closed: bool,

    session: Option<SessionSRTP>,
    ssrc: u32,

    buffer: Option<Buffer>,
}

pub struct ReadStreamSRTP {
    mu: Lock<ReadStreamSRTPInternal>,
}

impl ReadStreamSRTP {
    pub(crate) async fn init(&mut self, child: Session, ssrc: u32) -> Result<(), Error> {
        let session_rtp = match child {
            Session::SessionSRTP(s) => s,
            _ => {
                return Err(Error::new(
                    "ReadStreamSRTP init failed type assertion".to_string(),
                ))
            }
        };

        let mut r = self.mu.lock().await;
        if r.is_inited {
            return Err(Error::new(
                "ReadStreamSRTP has already been inited".to_string(),
            ));
        }

        r.session = Some(session_rtp);
        r.ssrc = ssrc;
        r.is_inited = true;

        // Create a buffer with a 1MB limit
        r.buffer = Some(Buffer::new(0, SRTP_BUFFER_SIZE));

        Ok(())
    }


    // Read reads and decrypts full RTP packet from the nextConn
    pub(crate) async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        let mut r = self.mu.lock().await;

        if let Some(buffer) = &mut r.buffer {
            buffer.read(buf).await
        } else {
            Err(Error::new("ReadStreamSRTP has empty buffer".to_string()))
        }
    }

    // GetSSRC returns the SSRC we are demuxing for
    pub(crate) async fn  get_ssrc(&mut self) ->u32 {
        let r = self.mu.lock().await;

        r.ssrc
    }

    pub(crate) fn new() -> Self {
        ReadStreamSRTP {
            mu: Lock::new(ReadStreamSRTPInternal {
                is_inited: false,
                is_closed: false,

                session: None,
                ssrc: 0,

                buffer: None,
            }),
        }
    }


    pub(crate) async fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        let mut r = self.mu.lock().await;

        if let Some(buffer) = &mut r.buffer {
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
        } else {
            Err(Error::new("ReadStreamSRTP has empty buffer".to_string()))
        }
    }


    // ReadRTP reads and decrypts full RTP packet and its header from the nextConn
    pub(crate) async fn read_rtp(&mut self, buf: &mut [u8]) -> Result<(usize, Header), Error> {
        let mut r = self.mu.lock().await;

        if let Some(buffer) = &mut r.buffer {
            let n = buffer.read(buf).await?;
            let mut reader = Cursor::new(buf);
            let header = Header::unmarshal(&mut reader)?;

            Ok((n, header))
        } else {
            Err(Error::new("ReadStreamSRTP has empty buffer".to_string()))
        }
    }

    // Close removes the ReadStream from the session and cleans up any associated state
    pub(crate) async fn close(&mut self) -> Result<(), Error> {
        let mut r = self.mu.lock().await;

        if !r.is_inited {
            return Err(Error::new("ReadStreamSRTP has not been inited".to_string()));
        }

        if r.is_closed {
            return Err(Error::new("ReadStreamSRTP is already closed".to_string()));
        }

        r.is_closed = true;

        if let Some(buffer) = &mut r.buffer {
            buffer.close().await;
        } else {
            return Err(Error::new("ReadStreamSRTP has empty buffer".to_string()));
        }

        let ssrc = r.ssrc;
        if let Some(session) = &mut r.session {
            session.session.remove_read_stream(ssrc);
        } else {
            return Err(Error::new("ReadStreamSRTP has empty session".to_string()));
        }

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