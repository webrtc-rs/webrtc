use crate::runtime::{AsyncTcpListener, AsyncTcpStream};
use rtc::shared::FourTuple;
use rtc::shared::tcp_framing::TcpFrameDecoder;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

pub(crate) struct RTCTcpTransport {
    pub(crate) listeners: HashMap<SocketAddr, Arc<dyn AsyncTcpListener>>,
    pub(crate) streams: HashMap<FourTuple, Arc<dyn AsyncTcpStream>>,
    pub(crate) decoders: HashMap<FourTuple, TcpFrameDecoder>,
}

impl RTCTcpTransport {
    pub(crate) fn new(tcp_listeners: HashMap<SocketAddr, Arc<dyn AsyncTcpListener>>) -> Self {
        Self {
            listeners: tcp_listeners,
            streams: HashMap::new(),
            decoders: HashMap::new(),
        }
    }
}
