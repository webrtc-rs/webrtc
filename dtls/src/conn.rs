use crate::curve::named_curve::NamedCurve;
use std::collections::HashMap;

use util::Error;

//pub(crate) initialTickerInterval = time.Second
pub(crate) const COOKIE_LENGTH: usize = 20;
pub(crate) const DEFAULT_NAMED_CURVE: NamedCurve = NamedCurve::X25519;
pub(crate) const INBOUND_BUFFER_SIZE: usize = 8192;
// Default replay protection window is specified by RFC 6347 Section 4.1.2.6
pub(crate) const DEFAULT_REPLAY_PROTECTION_WINDOW: usize = 64;

lazy_static! {
    pub static ref INVALID_KEYING_LABELS: HashMap<&'static str, bool> = {
        let mut map = HashMap::new();
        map.insert("client finished", true);
        map.insert("server finished", true);
        map.insert("master secret", true);
        map.insert("key expansion", true);
        map
    };
}

/*
pub(crate) trait FlightConn {
    fn notify(
        &self,
        /*ctx context.Context,*/ level: AlertLevel,
        desc: AlertDescription,
    ) -> Result<(), Error>;
    fn write_packets(&self, /*context.Context,*/ packets: Vec<Packet>) -> Result<(), Error>;
    fn recv_handshake(&self) -> mpsc::Receiver<()>;
    fn set_local_epoch(&self, epoch: u16);
    fn handle_queued_packets(&self /*context.Context*/) -> Result<(), Error>;
}*/

pub(crate) struct Conn;

impl Conn {
    pub(crate) fn handle_queued_packets(&self) -> Result<(), Error> {
        Ok(())
    }
}
