use super::*;
use crate::config::Config;
//use crate::stream::stream_srtp::WriteStreamSRTP;

use tokio::net::UdpSocket;

// SessionSRTP implements io.ReadWriteCloser and provides a bi-directional SRTP session
// SRTP itself does not have a design like this, but it is common in most applications
// for local/remote to each have their own keying material. This provides those patterns
// instead of making everyone re-implement
pub struct SessionSRTP {
    pub(crate) session: SessionBase,
    //pub(crate) write_stream: Option<WriteStreamSRTP<'a>>,
}

impl SessionSRTP {
    pub fn new(conn: UdpSocket, config: &Config) -> Result<Self, Error> {
        let mut s = SessionSRTP {
            session: SessionBase::new(conn),
            //write_stream: None,
        };

        //s.write_stream = Some(WriteStreamSRTP::new(&s));

        Ok(s)
    }
}
