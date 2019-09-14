use crate::session::Session;

use util::Error;

pub mod stream_srtcp;
pub mod stream_srtp;

use stream_srtcp::ReadStreamSRTCP;
use stream_srtp::ReadStreamSRTP;

pub enum ReadStream {
    ReadStreamSRTP(ReadStreamSRTP),
    ReadStreamSRTCP(ReadStreamSRTCP),
}

/*pub enum WriteStream {
    WriteStreamSRTP(WriteStreamSRTP),
    WriteStreamSRTCP(WriteStreamSRTCP),
}*/
