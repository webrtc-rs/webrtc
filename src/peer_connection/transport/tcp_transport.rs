use crate::peer_connection::driver::PeerConnectionDriverEvent;
use crate::peer_connection::transport::{TcpReadResult, is_retryable_socket_recv_error};
use crate::runtime::{AsyncTcpListener, AsyncTcpStream, Runtime, Sender};
use bytes::BytesMut;
use futures::FutureExt;
use futures::future::BoxFuture;
use futures::stream::FuturesUnordered;
use log::{error, trace};
use rtc::ice::candidate::Candidate;
use rtc::peer_connection::transport::{
    CandidateConfig, CandidateHostConfig, RTCIceCandidate, RTCIceCandidateInit,
};
use rtc::shared::FourTuple;
use rtc::shared::error::Result;
use rtc::shared::tcp_framing::{TcpFrameDecoder, frame_packet};
use rtc::shared::turn_framing::TurnStreamDecoder;
use rtc::shared::{TaggedBytesMut, TransportContext, TransportProtocol};
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;

const TCP_READ_BUF_LEN: usize = 4096;

pub(crate) type TcpAcceptResult = (
    SocketAddr,
    io::Result<(Arc<dyn AsyncTcpStream>, SocketAddr)>,
);

/// How a stream's bytes are cut into messages.
enum StreamDecoder {
    /// ICE-TCP: RFC 4571's two-byte length prefix before every packet.
    Rfc4571(TcpFrameDecoder),
    /// A connection to a TURN server: STUN and ChannelData back to back, each
    /// self-delimiting (RFC 8656 §12.5). Nothing is prefixed on the way out.
    Turn(TurnStreamDecoder),
}

pub(crate) struct RTCTcpTransport {
    listeners: HashMap<SocketAddr, Arc<dyn AsyncTcpListener>>,
    streams: HashMap<FourTuple, Arc<dyn AsyncTcpStream>>,
    decoders: HashMap<FourTuple, StreamDecoder>,
    /// TURN connections that ended, waiting for the driver to tell the relayer.
    closed_turn_streams: Vec<FourTuple>,
    /// TURN connections that are gone: a write to one is an error, never a silent
    /// drop and never a fallback onto some other stream.
    dead_turn_streams: HashSet<FourTuple>,
    pub(crate) accept_futures: FuturesUnordered<BoxFuture<'static, TcpAcceptResult>>,
    pub(crate) read_futures: FuturesUnordered<BoxFuture<'static, TcpReadResult>>,
}

impl RTCTcpTransport {
    pub(crate) fn new(tcp_listeners: HashMap<SocketAddr, Arc<dyn AsyncTcpListener>>) -> Self {
        let accept_futures = FuturesUnordered::new();
        for (local_addr, listener) in &tcp_listeners {
            let local_addr = *local_addr;
            let listener = listener.clone();
            accept_futures.push(
                async move {
                    match listener.accept().await {
                        Ok((stream, peer_addr)) => (local_addr, Ok((stream, peer_addr))),
                        Err(err) => (local_addr, Err(err)),
                    }
                }
                .boxed(),
            );
        }

        Self {
            listeners: tcp_listeners,
            streams: HashMap::new(),
            decoders: HashMap::new(),
            closed_turn_streams: Vec::new(),
            dead_turn_streams: HashSet::new(),
            accept_futures,
            read_futures: FuturesUnordered::new(),
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.listeners.is_empty()
    }

    pub(crate) fn listener_count(&self) -> usize {
        self.listeners.len()
    }

    pub(crate) fn has_stream_for(&self, four_tuple: &FourTuple) -> bool {
        self.streams.contains_key(four_tuple)
    }

    fn is_turn_stream(&self, four_tuple: &FourTuple) -> bool {
        matches!(self.decoders.get(four_tuple), Some(StreamDecoder::Turn(_)))
    }

    /// The stream for a four-tuple: the exact one, or for ICE-TCP any stream to the same
    /// peer. The fallback never picks a TURN connection — its bytes are framed differently,
    /// and ICE traffic has no business on it.
    fn find_stream(&self, four_tuple: &FourTuple) -> Option<Arc<dyn AsyncTcpStream>> {
        self.streams.get(four_tuple).cloned().or_else(|| {
            self.streams
                .iter()
                .find(|(tuple, stream)| {
                    !self.is_turn_stream(tuple)
                        && stream
                            .peer_addr()
                            .is_ok_and(|peer| peer == four_tuple.peer_addr)
                })
                .map(|(_, stream)| stream.clone())
        })
    }

    fn remove_stream(&mut self, four_tuple: &FourTuple) {
        self.streams.remove(four_tuple);
        if let Some(StreamDecoder::Turn(_)) = self.decoders.remove(four_tuple) {
            self.closed_turn_streams.push(*four_tuple);
            self.dead_turn_streams.insert(*four_tuple);
        }
    }

    /// Stop using a TURN connection the relayer no longer needs. Unlike a loss this is not
    /// reported back — the relayer asked for it.
    pub(crate) fn release_turn_stream(&mut self, four_tuple: &FourTuple) {
        if self.is_turn_stream(four_tuple) {
            self.streams.remove(four_tuple);
            self.decoders.remove(four_tuple);
            self.dead_turn_streams.insert(*four_tuple);
        }
    }

    /// TURN connections that have ended since the last call. The relayer drops the
    /// client that was using each one.
    pub(crate) fn take_closed_turn_streams(&mut self) -> Vec<FourTuple> {
        std::mem::take(&mut self.closed_turn_streams)
    }

    pub(crate) fn write<'a>(
        &self,
        msg: &'a TaggedBytesMut,
    ) -> Pin<Box<dyn Future<Output = Result<usize>> + Send + 'a>> {
        let four_tuple = FourTuple::from(&msg.transport);
        if self.dead_turn_streams.contains(&four_tuple) {
            return Box::pin(async move {
                Err(io::Error::new(
                    io::ErrorKind::NotConnected,
                    format!("TURN connection {four_tuple:?} has closed"),
                )
                .into())
            });
        }
        let Some(stream) = self.find_stream(&four_tuple) else {
            trace!("No TCP stream found for {:?}", four_tuple);
            return Box::pin(async { Ok(0) });
        };

        // TURN messages delimit themselves; only ICE-TCP gets a length prefix.
        let framed = if self.is_turn_stream(&four_tuple) {
            msg.message.to_vec()
        } else {
            frame_packet(&msg.message)
        };
        let len = msg.message.len();
        Box::pin(async move {
            stream.write_all(&framed).await?;
            Ok(len)
        })
    }

    fn arm_accept(&mut self, local_addr: SocketAddr) {
        if let Some(listener) = self.listeners.get(&local_addr).cloned() {
            self.accept_futures.push(
                async move {
                    match listener.accept().await {
                        Ok((stream, peer_addr)) => (local_addr, Ok((stream, peer_addr))),
                        Err(err) => (local_addr, Err(err)),
                    }
                }
                .boxed(),
            );
        }
    }

    fn arm_read(&mut self, four_tuple: FourTuple, stream: Arc<dyn AsyncTcpStream>) {
        self.read_futures.push(
            async move {
                let mut buf = vec![0u8; TCP_READ_BUF_LEN];
                match stream.read(&mut buf).await {
                    Ok(n) => TcpReadResult::Packet { four_tuple, n, buf },
                    Err(err) => TcpReadResult::Error {
                        four_tuple,
                        err,
                        buf,
                    },
                }
            }
            .boxed(),
        );
    }

    pub(crate) fn register_stream(
        &mut self,
        four_tuple: FourTuple,
        stream: Arc<dyn AsyncTcpStream>,
    ) {
        self.insert_stream(
            four_tuple,
            stream,
            StreamDecoder::Rfc4571(TcpFrameDecoder::new()),
        );
    }

    /// A connection to a TURN server, carrying STUN and ChannelData unprefixed.
    pub(crate) fn register_turn_stream(
        &mut self,
        four_tuple: FourTuple,
        stream: Arc<dyn AsyncTcpStream>,
    ) {
        self.insert_stream(
            four_tuple,
            stream,
            StreamDecoder::Turn(TurnStreamDecoder::new()),
        );
    }

    fn insert_stream(
        &mut self,
        four_tuple: FourTuple,
        stream: Arc<dyn AsyncTcpStream>,
        decoder: StreamDecoder,
    ) {
        self.dead_turn_streams.remove(&four_tuple);
        self.streams.insert(four_tuple, stream.clone());
        self.decoders.insert(four_tuple, decoder);
        self.arm_read(four_tuple, stream);
    }

    pub(crate) fn on_accept(
        &mut self,
        local_addr: SocketAddr,
        res: io::Result<(Arc<dyn AsyncTcpStream>, SocketAddr)>,
    ) -> Option<FourTuple> {
        let accepted = match res {
            Ok((stream, peer_addr)) => Some((stream, peer_addr)),
            Err(err) => {
                error!("TCP accept error: {}", err);
                None
            }
        };

        self.arm_accept(local_addr);

        let (stream, peer_addr) = accepted?;

        let stream_local_addr = stream.local_addr().unwrap_or(local_addr);
        let four_tuple = FourTuple {
            local_addr: stream_local_addr,
            peer_addr,
        };
        trace!(
            "Accepted TCP stream on {} from {}",
            stream_local_addr, peer_addr
        );
        self.register_stream(four_tuple, stream);
        Some(four_tuple)
    }

    /// `now` is when these bytes were observed; the caller supplies it so the packets carry the
    /// driver's clock rather than the wall clock.
    pub(crate) fn on_read(&mut self, now: Instant, res: TcpReadResult) -> Vec<TaggedBytesMut> {
        let mut out = Vec::new();
        match res {
            TcpReadResult::Packet { four_tuple, n, buf } => {
                if n == 0 {
                    trace!("TCP connection EOF for {:?}", four_tuple);
                    self.remove_stream(&four_tuple);
                } else {
                    let mut packets = Vec::new();
                    let mut broken = false;
                    match self.decoders.get_mut(&four_tuple) {
                        Some(StreamDecoder::Rfc4571(decoder)) => {
                            decoder.extend_from_slice(&buf[..n]);
                            while let Some(packet) = decoder.next_packet() {
                                packets.push(packet);
                            }
                        }
                        Some(StreamDecoder::Turn(decoder)) => {
                            decoder.extend_from_slice(&buf[..n]);
                            loop {
                                match decoder.next_message() {
                                    Ok(Some(message)) => packets.push(message),
                                    Ok(None) => break,
                                    // A stream has nowhere to resynchronise: whatever is
                                    // on it now is not TURN, and nothing after it will be.
                                    Err(err) => {
                                        error!("TURN stream {:?} is not TURN: {}", four_tuple, err);
                                        broken = true;
                                        break;
                                    }
                                }
                            }
                        }
                        None => {}
                    }
                    for packet in packets {
                        out.push(TaggedBytesMut {
                            now,
                            transport: TransportContext {
                                local_addr: four_tuple.local_addr,
                                peer_addr: four_tuple.peer_addr,
                                ecn: None,
                                transport_protocol: TransportProtocol::TCP,
                            },
                            message: BytesMut::from(&packet[..]),
                        });
                    }
                    if broken {
                        self.remove_stream(&four_tuple);
                    } else if let Some(stream) = self.streams.get(&four_tuple).cloned() {
                        self.arm_read(four_tuple, stream);
                    }
                }
            }
            TcpReadResult::Error {
                four_tuple,
                err,
                buf: _,
            } => {
                // A reset or refused connection is the end of a TURN connection, however
                // retryable it is on a datagram socket.
                if is_retryable_socket_recv_error(&err) && !self.is_turn_stream(&four_tuple) {
                    trace!("Transient TCP read error on {:?}: {}", four_tuple, err);
                    if let Some(stream) = self.streams.get(&four_tuple).cloned() {
                        self.arm_read(four_tuple, stream);
                    }
                } else {
                    error!("TCP read error on {:?}: {}", four_tuple, err);
                    self.remove_stream(&four_tuple);
                }
            }
        }
        out
    }

    pub(crate) fn gather_candidates(&self) -> Vec<RTCIceCandidateInit> {
        let mut candidates = Vec::new();
        for local_addr in self.listeners.keys() {
            // Gather passive TCP candidate
            let passive_config = CandidateHostConfig {
                base_config: CandidateConfig {
                    network: "tcp".to_owned(),
                    address: local_addr.ip().to_string(),
                    port: local_addr.port(),
                    component: 1,
                    ..Default::default()
                },
                tcp_type: rtc::ice::tcp_type::TcpType::Passive,
            };
            if let Ok(candidate) = passive_config.new_candidate_host()
                && let Ok(candidate_init) = RTCIceCandidate::from(&candidate).to_json()
            {
                candidates.push(candidate_init);
            }

            // Gather active TCP candidate
            let active_config = CandidateHostConfig {
                base_config: CandidateConfig {
                    network: "tcp".to_owned(),
                    address: local_addr.ip().to_string(),
                    port: 9, // Discard port placeholder for active candidates
                    component: 1,
                    ..Default::default()
                },
                tcp_type: rtc::ice::tcp_type::TcpType::Active,
            };
            if let Ok(candidate) = active_config.new_candidate_host()
                && let Ok(candidate_init) = RTCIceCandidate::from(&candidate).to_json()
            {
                candidates.push(candidate_init);
            }
        }
        candidates
    }

    pub(crate) fn connect(
        candidate: &Candidate,
        runtime: Arc<dyn Runtime>,
        tx: Sender<PeerConnectionDriverEvent>,
    ) {
        if candidate.network_type().is_tcp()
            && candidate.tcp_type() == rtc::ice::tcp_type::TcpType::Passive
            && let Ok(ip) = candidate.address().parse::<IpAddr>()
        {
            let remote_addr = SocketAddr::new(ip, candidate.port());
            let runtime_clone = runtime.clone();
            runtime.spawn(Box::pin(async move {
                trace!("Initiating TCP connect to {:?}", remote_addr);
                match runtime_clone.connect_tcp(remote_addr).await {
                    Ok(stream) => {
                        let local_addr = stream
                            .local_addr()
                            .unwrap_or_else(|_| "0.0.0.0:0".parse().unwrap());
                        let peer_addr = stream.peer_addr().unwrap_or(remote_addr);
                        let four_tuple = FourTuple {
                            local_addr,
                            peer_addr,
                        };
                        // overflow: detached — this is a per-candidate task spawned by
                        // `connect`, whose only remaining job is to hand the stream over. A
                        // full channel parks this task alone; the driver is unaffected.
                        let _ = tx
                            .send(PeerConnectionDriverEvent::IncomingTcpStream(
                                four_tuple, stream,
                            ))
                            .await;
                    }
                    Err(err) => {
                        error!("Failed to connect TCP to {:?}: {}", remote_addr, err);
                    }
                }
            }));
        }
    }
}

#[cfg(all(test, feature = "runtime-tokio"))]
mod tests {
    use super::*;
    use crate::runtime::default_runtime;
    use futures::StreamExt;

    /// Drive a future on the default runtime, returning its output.
    fn block_on<T>(future: impl Future<Output = T>) -> T {
        let mut out = None;
        {
            let slot = &mut out;
            default_runtime()
                .expect("a runtime")
                .block_on(Box::pin(async move {
                    *slot = Some(future.await);
                }));
        }
        out.expect("the future completed")
    }

    /// A connected pair: the transport's end, its four-tuple, and the far end.
    async fn connected_pair() -> (Arc<dyn AsyncTcpStream>, FourTuple, Arc<dyn AsyncTcpStream>) {
        let runtime = default_runtime().unwrap();
        let std_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = std_listener.local_addr().unwrap();
        let listener = runtime.wrap_tcp_listener(std_listener).unwrap();
        let (ours, theirs) =
            futures::future::join(runtime.connect_tcp(addr), listener.accept()).await;
        let ours = ours.unwrap();
        let (theirs, _) = theirs.unwrap();
        let four_tuple = FourTuple {
            local_addr: ours.local_addr().unwrap(),
            peer_addr: ours.peer_addr().unwrap(),
        };
        (ours, four_tuple, theirs)
    }

    fn outgoing(four_tuple: FourTuple, bytes: &[u8]) -> TaggedBytesMut {
        TaggedBytesMut {
            now: Instant::now(),
            transport: TransportContext {
                local_addr: four_tuple.local_addr,
                peer_addr: four_tuple.peer_addr,
                ecn: None,
                transport_protocol: TransportProtocol::TCP,
            },
            message: BytesMut::from(bytes),
        }
    }

    async fn read_exactly(stream: &Arc<dyn AsyncTcpStream>, n: usize) -> Vec<u8> {
        let mut got = Vec::new();
        while got.len() < n {
            let mut buf = vec![0u8; n - got.len()];
            let read = stream.read(&mut buf).await.unwrap();
            assert!(
                read > 0,
                "the stream closed after {} of {n} bytes",
                got.len()
            );
            got.extend_from_slice(&buf[..read]);
        }
        got
    }

    /// A ChannelData message on 0x4000 carrying three bytes, padded to 8.
    const CHANNEL_DATA: [u8; 8] = [0x40, 0x00, 0x00, 0x03, b'a', b'b', b'c', 0x00];
    /// A STUN header with no attributes.
    const STUN: [u8; 20] = [
        0x01, 0x01, 0x00, 0x00, 0x21, 0x12, 0xa4, 0x42, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12,
    ];

    #[test]
    fn a_turn_stream_carries_messages_without_a_length_prefix() {
        block_on(async {
            let (ours, four_tuple, theirs) = connected_pair().await;
            let mut transport = RTCTcpTransport::new(HashMap::new());
            transport.register_turn_stream(four_tuple, ours);

            let written = transport
                .write(&outgoing(four_tuple, &CHANNEL_DATA))
                .await
                .unwrap();
            assert_eq!(written, CHANNEL_DATA.len());
            assert_eq!(
                read_exactly(&theirs, CHANNEL_DATA.len()).await,
                CHANNEL_DATA,
                "TURN over TCP has no RFC 4571 prefix"
            );

            // Two messages in one write come back as two messages.
            theirs
                .write_all(&[STUN.as_slice(), CHANNEL_DATA.as_slice()].concat())
                .await
                .unwrap();
            let mut messages = Vec::new();
            while messages.len() < 2 {
                let res = transport.read_futures.next().await.unwrap();
                messages.extend(transport.on_read(Instant::now(), res));
            }
            assert_eq!(messages[0].message.as_ref(), STUN);
            assert_eq!(messages[1].message.as_ref(), CHANNEL_DATA);
            assert_eq!(FourTuple::from(&messages[0].transport), four_tuple);
            assert_eq!(
                messages[0].transport.transport_protocol,
                TransportProtocol::TCP
            );
        });
    }

    #[test]
    fn an_ice_tcp_stream_keeps_its_length_prefix() {
        block_on(async {
            let (ours, four_tuple, theirs) = connected_pair().await;
            let mut transport = RTCTcpTransport::new(HashMap::new());
            transport.register_stream(four_tuple, ours);
            transport.write(&outgoing(four_tuple, &STUN)).await.unwrap();
            let got = read_exactly(&theirs, 2 + STUN.len()).await;
            assert_eq!(&got[..2], &(STUN.len() as u16).to_be_bytes());
            assert_eq!(&got[2..], STUN);
        });
    }

    #[test]
    fn a_turn_stream_that_ends_is_reported_and_its_writes_fail() {
        block_on(async {
            let (ours, four_tuple, theirs) = connected_pair().await;
            let mut transport = RTCTcpTransport::new(HashMap::new());
            transport.register_turn_stream(four_tuple, ours);
            assert!(transport.take_closed_turn_streams().is_empty());

            drop(theirs);
            let res = transport.read_futures.next().await.unwrap();
            assert!(transport.on_read(Instant::now(), res).is_empty());
            assert_eq!(
                transport.take_closed_turn_streams(),
                vec![four_tuple],
                "the relayer has to hear that its connection is gone"
            );
            assert!(
                transport.write(&outgoing(four_tuple, &STUN)).await.is_err(),
                "a write to a dead TURN stream fails rather than vanishing"
            );
        });
    }

    #[test]
    fn a_turn_stream_that_is_not_carrying_turn_is_closed() {
        block_on(async {
            let (ours, four_tuple, theirs) = connected_pair().await;
            let mut transport = RTCTcpTransport::new(HashMap::new());
            transport.register_turn_stream(four_tuple, ours);
            // An RFC 4571 prefix is not a STUN or ChannelData header.
            theirs
                .write_all(&[0x80, 0x00, 0x00, 0x04, 1, 2, 3, 4])
                .await
                .unwrap();
            let res = transport.read_futures.next().await.unwrap();
            assert!(transport.on_read(Instant::now(), res).is_empty());
            assert_eq!(transport.take_closed_turn_streams(), vec![four_tuple]);
        });
    }

    #[test]
    fn a_released_turn_stream_is_not_reported_as_lost() {
        block_on(async {
            let (ours, four_tuple, _theirs) = connected_pair().await;
            let mut transport = RTCTcpTransport::new(HashMap::new());
            transport.register_turn_stream(four_tuple, ours);
            // The relayer let go of it: closing it is not news to anyone.
            transport.release_turn_stream(&four_tuple);
            assert!(transport.take_closed_turn_streams().is_empty());
            assert!(
                transport.write(&outgoing(four_tuple, &STUN)).await.is_err(),
                "nothing more goes out on it"
            );
        });
    }

    #[test]
    fn ice_traffic_never_falls_back_onto_a_turn_stream() {
        block_on(async {
            let (ours, turn_tuple, theirs) = connected_pair().await;
            let mut transport = RTCTcpTransport::new(HashMap::new());
            transport.register_turn_stream(turn_tuple, ours);
            // Same peer, different local address: the peer-address fallback that ICE-TCP
            // relies on must not pick the TURN connection.
            let stranger = FourTuple {
                local_addr: "127.0.0.1:9".parse().unwrap(),
                peer_addr: turn_tuple.peer_addr,
            };
            let written = transport.write(&outgoing(stranger, &STUN)).await.unwrap();
            assert_eq!(written, 0, "nothing was sent");
            let mut probe = [0u8; 1];
            let read = futures::future::select(
                Box::pin(theirs.read(&mut probe)),
                Box::pin(
                    default_runtime()
                        .unwrap()
                        .sleep(std::time::Duration::from_millis(200)),
                ),
            )
            .await;
            assert!(
                matches!(read, futures::future::Either::Right(_)),
                "bytes reached the TURN server that were not its"
            );
        });
    }
}
