use crate::session::Session;

use util::Error;

use tokio::sync::mpsc;

pub mod stream_srtcp;
pub mod stream_srtp;

pub trait ReadStream {
    fn init(&self, child: Session, ssrc: u32) -> Result<(), Error>;
    fn read(&self, buf: &[u8]) -> Result<isize, Error>;
    fn get_ssrc(&self) -> u32;
}

// Limit the buffer size to 1MB
const srtpBufferSize: usize = 1000 * 1000;

// ReadStreamSRTP handles decryption for a single RTP SSRC
struct ReadStreamSRTP {
    //mu sync.Mutex
    is_inited: bool,

    is_closed_tx: Option<mpsc::Sender<bool>>,
    is_closed_rx: mpsc::Receiver<bool>,

    session: Option<Session>,
    ssrc: u32,
    //buffer: *packetio.Buffer
}
