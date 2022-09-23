use super::*;
use crate::association::Event;
use crate::error::{Error, Result};

use crate::association::state::{AckMode, AssociationState};
use crate::association::stream::{ReliabilityType, Stream};
use crate::chunk::chunk_abort::ChunkAbort;
use crate::chunk::chunk_cookie_echo::ChunkCookieEcho;
use crate::chunk::chunk_error::ChunkError;
use crate::chunk::chunk_forward_tsn::ChunkForwardTsn;
use crate::chunk::chunk_heartbeat::ChunkHeartbeat;
use crate::chunk::chunk_init::ChunkInit;
use crate::chunk::chunk_payload_data::{ChunkPayloadData, PayloadProtocolIdentifier};
use crate::chunk::chunk_reconfig::ChunkReconfig;
use crate::chunk::chunk_selective_ack::{ChunkSelectiveAck, GapAckBlock};
use crate::chunk::chunk_shutdown::ChunkShutdown;
use crate::chunk::chunk_shutdown_ack::ChunkShutdownAck;
use crate::chunk::chunk_shutdown_complete::ChunkShutdownComplete;
use crate::chunk::{ErrorCauseProtocolViolation, PROTOCOL_VIOLATION};
use crate::packet::{CommonHeader, Packet};
use crate::param::param_outgoing_reset_request::ParamOutgoingResetRequest;
use crate::param::param_reconfig_response::ParamReconfigResponse;
use assert_matches::assert_matches;
use lazy_static::lazy_static;
use log::{info, trace};
use std::net::Ipv6Addr;
use std::ops::RangeFrom;
use std::str::FromStr;
use std::sync::Mutex;
use std::{cmp, mem, net::UdpSocket, time::Duration};

lazy_static! {
    pub static ref SERVER_PORTS: Mutex<RangeFrom<u16>> = Mutex::new(4433..);
    pub static ref CLIENT_PORTS: Mutex<RangeFrom<u16>> = Mutex::new(44433..);
}

fn min_opt<T: Ord>(x: Option<T>, y: Option<T>) -> Option<T> {
    match (x, y) {
        (Some(x), Some(y)) => Some(cmp::min(x, y)),
        (Some(x), _) => Some(x),
        (_, Some(y)) => Some(y),
        _ => None,
    }
}

/// The maximum of datagrams TestEndpoint will produce via `poll_transmit`
const MAX_DATAGRAMS: usize = 10;

fn split_transmit(transmit: Transmit) -> Vec<Transmit> {
    let mut transmits = Vec::new();
    if let Payload::RawEncode(contents) = transmit.payload {
        for content in contents {
            transmits.push(Transmit {
                now: transmit.now,
                remote: transmit.remote,
                payload: Payload::RawEncode(vec![content]),
                ecn: transmit.ecn,
                local_ip: transmit.local_ip,
            });
        }
    }

    transmits
}

pub fn client_config() -> ClientConfig {
    ClientConfig::new()
}

pub fn server_config() -> ServerConfig {
    ServerConfig::new()
}

struct TestEndpoint {
    endpoint: Endpoint,
    addr: SocketAddr,
    socket: Option<UdpSocket>,
    timeout: Option<Instant>,
    outbound: VecDeque<Transmit>,
    delayed: VecDeque<Transmit>,
    inbound: VecDeque<(Instant, Option<EcnCodepoint>, Bytes)>,
    accepted: Option<AssociationHandle>,
    associations: HashMap<AssociationHandle, Association>,
    conn_events: HashMap<AssociationHandle, VecDeque<AssociationEvent>>,
}

impl TestEndpoint {
    fn new(endpoint: Endpoint, addr: SocketAddr) -> Self {
        let socket = UdpSocket::bind(addr).expect("failed to bind UDP socket");
        socket
            .set_read_timeout(Some(Duration::new(0, 10_000_000)))
            .unwrap();

        Self {
            endpoint,
            addr,
            socket: Some(socket),
            timeout: None,
            outbound: VecDeque::new(),
            delayed: VecDeque::new(),
            inbound: VecDeque::new(),
            accepted: None,
            associations: HashMap::default(),
            conn_events: HashMap::default(),
        }
    }

    pub fn drive(&mut self, now: Instant, remote: SocketAddr) {
        if let Some(ref socket) = self.socket {
            loop {
                let mut buf = [0; 8192];
                if socket.recv_from(&mut buf).is_err() {
                    break;
                }
            }
        }

        while self.inbound.front().map_or(false, |x| x.0 <= now) {
            let (recv_time, ecn, packet) = self.inbound.pop_front().unwrap();
            if let Some((ch, event)) =
                self.endpoint
                    .handle(recv_time, remote, None, ecn, packet.into())
            {
                match event {
                    DatagramEvent::NewAssociation(conn) => {
                        self.associations.insert(ch, conn);
                        self.accepted = Some(ch);
                    }
                    DatagramEvent::AssociationEvent(event) => {
                        self.conn_events
                            .entry(ch)
                            .or_insert_with(VecDeque::new)
                            .push_back(event);
                    }
                }
            }
        }

        while let Some(x) = self.poll_transmit() {
            self.outbound.extend(split_transmit(x));
        }

        let mut endpoint_events: Vec<(AssociationHandle, EndpointEvent)> = vec![];
        for (ch, conn) in self.associations.iter_mut() {
            if self.timeout.map_or(false, |x| x <= now) {
                self.timeout = None;
                conn.handle_timeout(now);
            }

            for (_, mut events) in self.conn_events.drain() {
                for event in events.drain(..) {
                    conn.handle_event(event);
                }
            }

            while let Some(event) = conn.poll_endpoint_event() {
                endpoint_events.push((*ch, event));
            }

            while let Some(x) = conn.poll_transmit(now) {
                self.outbound.extend(split_transmit(x));
            }
            self.timeout = conn.poll_timeout();
        }

        for (ch, event) in endpoint_events {
            if let Some(event) = self.handle_event(ch, event) {
                if let Some(conn) = self.associations.get_mut(&ch) {
                    conn.handle_event(event);
                }
            }
        }
    }

    pub fn next_wakeup(&self) -> Option<Instant> {
        let next_inbound = self.inbound.front().map(|x| x.0);
        min_opt(self.timeout, next_inbound)
    }

    fn is_idle(&self) -> bool {
        self.associations.values().all(|x| x.is_idle())
    }

    pub fn delay_outbound(&mut self) {
        assert!(self.delayed.is_empty());
        mem::swap(&mut self.delayed, &mut self.outbound);
    }

    pub fn finish_delay(&mut self) {
        self.outbound.extend(self.delayed.drain(..));
    }

    pub fn assert_accept(&mut self) -> AssociationHandle {
        self.accepted.take().expect("server didn't connect")
    }
}

impl ::std::ops::Deref for TestEndpoint {
    type Target = Endpoint;
    fn deref(&self) -> &Endpoint {
        &self.endpoint
    }
}

impl ::std::ops::DerefMut for TestEndpoint {
    fn deref_mut(&mut self) -> &mut Endpoint {
        &mut self.endpoint
    }
}

struct Pair {
    server: TestEndpoint,
    client: TestEndpoint,
    time: Instant,
    latency: Duration, // One-way
}

impl Pair {
    pub fn new(endpoint_config: Arc<EndpointConfig>, server_config: ServerConfig) -> Self {
        let server = Endpoint::new(endpoint_config.clone(), Some(Arc::new(server_config)));
        let client = Endpoint::new(endpoint_config, None);

        Pair::new_from_endpoint(client, server)
    }

    pub fn new_from_endpoint(client: Endpoint, server: Endpoint) -> Self {
        let server_addr = SocketAddr::new(
            Ipv6Addr::LOCALHOST.into(),
            SERVER_PORTS.lock().unwrap().next().unwrap(),
        );
        let client_addr = SocketAddr::new(
            Ipv6Addr::LOCALHOST.into(),
            CLIENT_PORTS.lock().unwrap().next().unwrap(),
        );
        Self {
            server: TestEndpoint::new(server, server_addr),
            client: TestEndpoint::new(client, client_addr),
            time: Instant::now(),
            latency: Duration::new(0, 0),
        }
    }

    /// Returns whether the association is not idle
    pub fn step(&mut self) -> bool {
        self.drive_client();
        self.drive_server();
        if self.client.is_idle() && self.server.is_idle() {
            return false;
        }

        let client_t = self.client.next_wakeup();
        let server_t = self.server.next_wakeup();
        match min_opt(client_t, server_t) {
            Some(t) if Some(t) == client_t => {
                if t != self.time {
                    self.time = self.time.max(t);
                    trace!("advancing to {:?} for client", self.time);
                }
                true
            }
            Some(t) if Some(t) == server_t => {
                if t != self.time {
                    self.time = self.time.max(t);
                    trace!("advancing to {:?} for server", self.time);
                }
                true
            }
            Some(_) => unreachable!(),
            None => false,
        }
    }

    /// Advance time until both associations are idle
    pub fn drive(&mut self) {
        while self.step() {}
    }

    pub fn drive_client(&mut self) {
        self.client.drive(self.time, self.server.addr);
        for x in self.client.outbound.drain(..) {
            if let Payload::RawEncode(contents) = x.payload {
                for content in contents {
                    if let Some(ref socket) = self.client.socket {
                        socket.send_to(&content, x.remote).unwrap();
                    }
                    if self.server.addr == x.remote {
                        self.server
                            .inbound
                            .push_back((self.time + self.latency, x.ecn, content));
                    }
                }
            }
        }
    }

    pub fn drive_server(&mut self) {
        self.server.drive(self.time, self.client.addr);
        for x in self.server.outbound.drain(..) {
            if let Payload::RawEncode(contents) = x.payload {
                for content in contents {
                    if let Some(ref socket) = self.server.socket {
                        socket.send_to(&content, x.remote).unwrap();
                    }
                    if self.client.addr == x.remote {
                        self.client
                            .inbound
                            .push_back((self.time + self.latency, x.ecn, content));
                    }
                }
            }
        }
    }

    pub fn connect(&mut self) -> (AssociationHandle, AssociationHandle) {
        self.connect_with(client_config())
    }

    pub fn connect_with(&mut self, config: ClientConfig) -> (AssociationHandle, AssociationHandle) {
        info!("connecting");
        let client_ch = self.begin_connect(config);
        self.drive();
        let server_ch = self.server.assert_accept();
        self.finish_connect(client_ch, server_ch);
        (client_ch, server_ch)
    }

    /// Just start connecting the client
    pub fn begin_connect(&mut self, config: ClientConfig) -> AssociationHandle {
        let (client_ch, client_conn) = self.client.connect(config, self.server.addr).unwrap();
        self.client.associations.insert(client_ch, client_conn);
        client_ch
    }

    fn finish_connect(&mut self, client_ch: AssociationHandle, server_ch: AssociationHandle) {
        assert_matches!(
            self.client_conn_mut(client_ch).poll(),
            Some(Event::Connected { .. })
        );

        assert_matches!(
            self.server_conn_mut(server_ch).poll(),
            Some(Event::Connected { .. })
        );
    }

    pub fn client_conn_mut(&mut self, ch: AssociationHandle) -> &mut Association {
        self.client.associations.get_mut(&ch).unwrap()
    }

    pub fn client_stream(&mut self, ch: AssociationHandle, si: u16) -> Result<Stream<'_>> {
        self.client_conn_mut(ch).stream(si)
    }

    pub fn server_conn_mut(&mut self, ch: AssociationHandle) -> &mut Association {
        self.server.associations.get_mut(&ch).unwrap()
    }

    pub fn server_stream(&mut self, ch: AssociationHandle, si: u16) -> Result<Stream<'_>> {
        self.server_conn_mut(ch).stream(si)
    }
}

impl Default for Pair {
    fn default() -> Self {
        Pair::new(Default::default(), server_config())
    }
}

fn create_association_pair(
    ack_mode: AckMode,
    recv_buf_size: u32,
) -> Result<(Pair, AssociationHandle, AssociationHandle)> {
    let mut pair = Pair::new(
        Arc::new(EndpointConfig::default()),
        ServerConfig {
            transport: Arc::new(if recv_buf_size > 0 {
                TransportConfig::default().with_max_receive_buffer_size(recv_buf_size)
            } else {
                TransportConfig::default()
            }),
            ..Default::default()
        },
    );
    let (client_ch, server_ch) = pair.connect_with(ClientConfig {
        transport: Arc::new(if recv_buf_size > 0 {
            TransportConfig::default().with_max_receive_buffer_size(recv_buf_size)
        } else {
            TransportConfig::default()
        }),
    });
    pair.client_conn_mut(client_ch).ack_mode = ack_mode;
    pair.server_conn_mut(server_ch).ack_mode = ack_mode;
    Ok((pair, client_ch, server_ch))
}

fn establish_session_pair(
    pair: &mut Pair,
    client_ch: AssociationHandle,
    server_ch: AssociationHandle,
    si: u16,
) -> Result<()> {
    let hello_msg = Bytes::from_static(b"Hello");
    let _ = pair
        .client_conn_mut(client_ch)
        .open_stream(si, PayloadProtocolIdentifier::Binary)?;
    let _ = pair
        .client_stream(client_ch, si)?
        .write_sctp(&hello_msg, PayloadProtocolIdentifier::Dcep)?;
    pair.drive();

    {
        let s1 = pair.server_conn_mut(server_ch).accept_stream().unwrap();
        if si != s1.stream_identifier {
            return Err(Error::Other("si should match".to_owned()).into());
        }
    }
    pair.drive();

    let mut buf = vec![0u8; 1024];
    let chunks = pair.server_stream(server_ch, si)?.read_sctp()?.unwrap();
    let n = chunks.read(&mut buf)?;

    if n != hello_msg.len() {
        return Err(Error::Other("received data must by 3 bytes".to_owned()).into());
    }

    if chunks.ppi != PayloadProtocolIdentifier::Dcep {
        return Err(Error::Other("unexpected ppi".to_owned()).into());
    }

    if &buf[..n] != &hello_msg {
        return Err(Error::Other("received data mismatch".to_owned()).into());
    }
    pair.drive();

    Ok(())
}

fn close_association_pair(
    _pair: &mut Pair,
    _client_ch: AssociationHandle,
    _server_ch: AssociationHandle,
    _si: u16,
) {
    /*TODO:
    // Close client
    tokio::spawn(async move {
        client.close().await?;
        let _ = handshake0ch_tx.send(()).await;
        let _ = closed_rx0.recv().await;

        Result::<()>::Ok(())
    });

    // Close server
    tokio::spawn(async move {
        server.close().await?;
        let _ = handshake1ch_tx.send(()).await;
        let _ = closed_rx1.recv().await;

        Result::<()>::Ok(())
    });
    */
}

#[test]
fn test_assoc_reliable_simple() -> Result<()> {
    //let _guard = subscribe();

    let si: u16 = 1;
    let msg: Bytes = Bytes::from_static(b"ABC");

    let (mut pair, client_ch, server_ch) = create_association_pair(AckMode::NoDelay, 0)?;

    establish_session_pair(&mut pair, client_ch, server_ch, si)?;

    {
        let a = pair.client_conn_mut(client_ch);
        assert_eq!(0, a.buffered_amount(), "incorrect bufferedAmount");
    }

    let n = pair
        .client_stream(client_ch, si)?
        .write_sctp(&msg, PayloadProtocolIdentifier::Binary)?;
    assert_eq!(msg.len(), n, "unexpected length of received data");
    {
        let a = pair.client_conn_mut(client_ch);
        assert_eq!(msg.len(), a.buffered_amount(), "incorrect bufferedAmount");
    }

    pair.drive();

    let chunks = pair.server_stream(server_ch, si)?.read_sctp()?.unwrap();
    let (n, ppi) = (chunks.len(), chunks.ppi);
    assert_eq!(n, msg.len(), "unexpected length of received data");
    assert_eq!(ppi, PayloadProtocolIdentifier::Binary, "unexpected ppi");

    {
        let q = &pair
            .client_conn_mut(client_ch)
            .streams
            .get(&si)
            .unwrap()
            .reassembly_queue;
        assert!(!q.is_readable(), "should no longer be readable");
    }

    {
        let a = pair.client_conn_mut(client_ch);
        assert_eq!(0, a.buffered_amount(), "incorrect bufferedAmount");
    }

    close_association_pair(&mut pair, client_ch, server_ch, si);

    Ok(())
}

#[test]
fn test_assoc_reliable_ordered_reordered() -> Result<()> {
    // let _guard = subscribe();

    let si: u16 = 2;
    let mut sbuf = vec![0u8; 1000];
    for i in 0..sbuf.len() {
        sbuf[i] = (i & 0xff) as u8;
    }

    let (mut pair, client_ch, server_ch) = create_association_pair(AckMode::NoDelay, 0)?;

    establish_session_pair(&mut pair, client_ch, server_ch, si)?;

    {
        let a = pair.client_conn_mut(client_ch);
        assert_eq!(0, a.buffered_amount(), "incorrect bufferedAmount");
    }

    sbuf[0..4].copy_from_slice(&0u32.to_be_bytes());
    let n = pair.client_stream(client_ch, si)?.write_sctp(
        &Bytes::from(sbuf.clone()),
        PayloadProtocolIdentifier::Binary,
    )?;
    assert_eq!(sbuf.len(), n, "unexpected length of received data");
    pair.client.drive(pair.time, pair.server.addr);
    pair.client.delay_outbound(); // Delay it

    sbuf[0..4].copy_from_slice(&1u32.to_be_bytes());
    let n = pair.client_stream(client_ch, si)?.write_sctp(
        &Bytes::from(sbuf.clone()),
        PayloadProtocolIdentifier::Binary,
    )?;
    assert_eq!(sbuf.len(), n, "unexpected length of received data");
    pair.client.drive(pair.time, pair.server.addr);
    pair.client.finish_delay(); // Reorder it

    pair.drive();

    let mut buf = vec![0u8; 2000];

    let chunks = pair.server_stream(server_ch, si)?.read_sctp()?.unwrap();
    let (n, ppi) = (chunks.len(), chunks.ppi);
    chunks.read(&mut buf)?;
    assert_eq!(n, sbuf.len(), "unexpected length of received data");
    assert_eq!(ppi, PayloadProtocolIdentifier::Binary, "unexpected ppi");
    assert_eq!(
        u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]),
        0,
        "unexpected received data"
    );

    let chunks = pair.server_stream(server_ch, si)?.read_sctp()?.unwrap();
    let (n, ppi) = (chunks.len(), chunks.ppi);
    chunks.read(&mut buf)?;
    assert_eq!(n, sbuf.len(), "unexpected length of received data");
    assert_eq!(ppi, PayloadProtocolIdentifier::Binary, "unexpected ppi");
    assert_eq!(
        u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]),
        1,
        "unexpected received data"
    );

    pair.drive();

    {
        let q = &pair
            .client_conn_mut(client_ch)
            .streams
            .get(&si)
            .unwrap()
            .reassembly_queue;
        assert!(!q.is_readable(), "should no longer be readable");
    }

    close_association_pair(&mut pair, client_ch, server_ch, si);

    Ok(())
}

#[test]
fn test_assoc_reliable_ordered_fragmented_then_defragmented() -> Result<()> {
    //let _guard = subscribe();

    let si: u16 = 3;
    let mut sbuf = vec![0u8; 1000];
    for i in 0..sbuf.len() {
        sbuf[i] = (i & 0xff) as u8;
    }
    let mut sbufl = vec![0u8; 2000];
    for i in 0..sbufl.len() {
        sbufl[i] = (i & 0xff) as u8;
    }

    let (mut pair, client_ch, server_ch) = create_association_pair(AckMode::NoDelay, 0)?;

    establish_session_pair(&mut pair, client_ch, server_ch, si)?;

    pair.client_stream(client_ch, si)?.set_reliability_params(
        false,
        ReliabilityType::Reliable,
        0,
    )?;
    pair.server_stream(server_ch, si)?.set_reliability_params(
        false,
        ReliabilityType::Reliable,
        0,
    )?;

    let n = pair.client_stream(client_ch, si)?.write_sctp(
        &Bytes::from(sbufl.clone()),
        PayloadProtocolIdentifier::Binary,
    )?;
    assert_eq!(sbufl.len(), n, "unexpected length of received data");

    pair.drive();

    let mut rbuf = vec![0u8; 2000];
    let chunks = pair.server_stream(server_ch, si)?.read_sctp()?.unwrap();
    let (n, ppi) = (chunks.len(), chunks.ppi);
    chunks.read(&mut rbuf)?;
    assert_eq!(n, sbufl.len(), "unexpected length of received data");
    assert_eq!(&rbuf[..n], &sbufl, "unexpected received data");
    assert_eq!(ppi, PayloadProtocolIdentifier::Binary, "unexpected ppi");

    pair.drive();

    {
        let q = &pair
            .client_conn_mut(client_ch)
            .streams
            .get(&si)
            .unwrap()
            .reassembly_queue;
        assert!(!q.is_readable(), "should no longer be readable");
    }

    close_association_pair(&mut pair, client_ch, server_ch, si);

    Ok(())
}

#[test]
fn test_assoc_reliable_unordered_fragmented_then_defragmented() -> Result<()> {
    //let _guard = subscribe();

    let si: u16 = 4;
    let mut sbuf = vec![0u8; 1000];
    for i in 0..sbuf.len() {
        sbuf[i] = (i & 0xff) as u8;
    }
    let mut sbufl = vec![0u8; 2000];
    for i in 0..sbufl.len() {
        sbufl[i] = (i & 0xff) as u8;
    }

    let (mut pair, client_ch, server_ch) = create_association_pair(AckMode::NoDelay, 0)?;

    establish_session_pair(&mut pair, client_ch, server_ch, si)?;

    pair.client_stream(client_ch, si)?.set_reliability_params(
        true,
        ReliabilityType::Reliable,
        0,
    )?;
    pair.server_stream(server_ch, si)?.set_reliability_params(
        true,
        ReliabilityType::Reliable,
        0,
    )?;

    let n = pair.client_stream(client_ch, si)?.write_sctp(
        &Bytes::from(sbufl.clone()),
        PayloadProtocolIdentifier::Binary,
    )?;
    assert_eq!(sbufl.len(), n, "unexpected length of received data");

    pair.drive();

    let mut rbuf = vec![0u8; 2000];
    let chunks = pair.server_stream(server_ch, si)?.read_sctp()?.unwrap();
    let (n, ppi) = (chunks.len(), chunks.ppi);
    chunks.read(&mut rbuf)?;
    assert_eq!(n, sbufl.len(), "unexpected length of received data");
    assert_eq!(&rbuf[..n], &sbufl, "unexpected received data");
    assert_eq!(ppi, PayloadProtocolIdentifier::Binary, "unexpected ppi");

    pair.drive();

    {
        let q = &pair
            .client_conn_mut(client_ch)
            .streams
            .get(&si)
            .unwrap()
            .reassembly_queue;
        assert!(!q.is_readable(), "should no longer be readable");
    }

    close_association_pair(&mut pair, client_ch, server_ch, si);

    Ok(())
}

#[test]
fn test_assoc_reliable_unordered_ordered() -> Result<()> {
    //let _guard = subscribe();

    let si: u16 = 5;
    let mut sbuf = vec![0u8; 1000];
    for i in 0..sbuf.len() {
        sbuf[i] = (i & 0xff) as u8;
    }

    let (mut pair, client_ch, server_ch) = create_association_pair(AckMode::NoDelay, 0)?;

    establish_session_pair(&mut pair, client_ch, server_ch, si)?;

    pair.client_stream(client_ch, si)?.set_reliability_params(
        true,
        ReliabilityType::Reliable,
        0,
    )?;
    pair.server_stream(server_ch, si)?.set_reliability_params(
        true,
        ReliabilityType::Reliable,
        0,
    )?;

    sbuf[0..4].copy_from_slice(&0u32.to_be_bytes());
    let n = pair.client_stream(client_ch, si)?.write_sctp(
        &Bytes::from(sbuf.clone()),
        PayloadProtocolIdentifier::Binary,
    )?;
    assert_eq!(sbuf.len(), n, "unexpected length of received data");
    pair.client.drive(pair.time, pair.server.addr);
    pair.client.delay_outbound(); // Delay it

    sbuf[0..4].copy_from_slice(&1u32.to_be_bytes());
    let n = pair.client_stream(client_ch, si)?.write_sctp(
        &Bytes::from(sbuf.clone()),
        PayloadProtocolIdentifier::Binary,
    )?;
    assert_eq!(sbuf.len(), n, "unexpected length of received data");
    pair.client.drive(pair.time, pair.server.addr);
    pair.client.finish_delay(); // Reorder it

    pair.drive();

    let mut buf = vec![0u8; 2000];

    let chunks = pair.server_stream(server_ch, si)?.read_sctp()?.unwrap();
    let (n, ppi) = (chunks.len(), chunks.ppi);
    chunks.read(&mut buf)?;
    assert_eq!(n, sbuf.len(), "unexpected length of received data");
    assert_eq!(ppi, PayloadProtocolIdentifier::Binary, "unexpected ppi");
    assert_eq!(
        u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]),
        1,
        "unexpected received data"
    );

    let chunks = pair.server_stream(server_ch, si)?.read_sctp()?.unwrap();
    let (n, ppi) = (chunks.len(), chunks.ppi);
    chunks.read(&mut buf)?;
    assert_eq!(n, sbuf.len(), "unexpected length of received data");
    assert_eq!(ppi, PayloadProtocolIdentifier::Binary, "unexpected ppi");
    assert_eq!(
        u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]),
        0,
        "unexpected received data"
    );

    pair.drive();

    {
        let q = &pair
            .client_conn_mut(client_ch)
            .streams
            .get(&si)
            .unwrap()
            .reassembly_queue;
        assert!(!q.is_readable(), "should no longer be readable");
    }

    close_association_pair(&mut pair, client_ch, server_ch, si);

    Ok(())
}

#[test]
fn test_assoc_reliable_retransmission() -> Result<()> {
    //let _guard = subscribe();

    let si: u16 = 6;
    let msg1: Bytes = Bytes::from_static(b"ABC");
    let msg2: Bytes = Bytes::from_static(b"DEFG");

    let (mut pair, client_ch, server_ch) = create_association_pair(AckMode::NoDelay, 0)?;

    {
        let a = pair.client_conn_mut(client_ch);
        a.rto_mgr.set_rto(100, true);
    }

    establish_session_pair(&mut pair, client_ch, server_ch, si)?;

    let n = pair
        .client_stream(client_ch, si)?
        .write_sctp(&msg1, PayloadProtocolIdentifier::Binary)?;
    assert_eq!(msg1.len(), n, "unexpected length of received data");
    pair.drive_client(); // send data to server
    pair.server.inbound.clear(); // Lose it
    debug!("dropping packet");

    let n = pair
        .client_stream(client_ch, si)?
        .write_sctp(&msg2, PayloadProtocolIdentifier::Binary)?;
    assert_eq!(msg2.len(), n, "unexpected length of received data");

    pair.drive();

    let mut buf = vec![0u8; 32];

    let chunks = pair.server_stream(server_ch, si)?.read_sctp()?.unwrap();
    let (n, ppi) = (chunks.len(), chunks.ppi);
    chunks.read(&mut buf)?;
    assert_eq!(n, msg1.len(), "unexpected length of received data");
    assert_eq!(&buf[..n], &msg1, "unexpected length of received data");
    assert_eq!(ppi, PayloadProtocolIdentifier::Binary, "unexpected ppi");

    let chunks = pair.server_stream(server_ch, si)?.read_sctp()?.unwrap();
    let (n, ppi) = (chunks.len(), chunks.ppi);
    chunks.read(&mut buf)?;
    assert_eq!(n, msg2.len(), "unexpected length of received data");
    assert_eq!(&buf[..n], &msg2, "unexpected length of received data");
    assert_eq!(ppi, PayloadProtocolIdentifier::Binary, "unexpected ppi");

    pair.drive();

    {
        let q = &pair
            .client_conn_mut(client_ch)
            .streams
            .get(&si)
            .unwrap()
            .reassembly_queue;
        assert!(!q.is_readable(), "should no longer be readable");
    }

    close_association_pair(&mut pair, client_ch, server_ch, si);

    Ok(())
}

#[test]
fn test_assoc_reliable_short_buffer() -> Result<()> {
    //let _guard = subscribe();

    let si: u16 = 1;
    let msg: Bytes = Bytes::from_static(b"Hello");

    let (mut pair, client_ch, server_ch) = create_association_pair(AckMode::NoDelay, 0)?;

    establish_session_pair(&mut pair, client_ch, server_ch, si)?;

    {
        let a = pair.client_conn_mut(client_ch);
        assert_eq!(0, a.buffered_amount(), "incorrect bufferedAmount");
    }

    let n = pair
        .client_stream(client_ch, si)?
        .write_sctp(&msg, PayloadProtocolIdentifier::Binary)?;
    assert_eq!(msg.len(), n, "unexpected length of received data");
    {
        let a = pair.client_conn_mut(client_ch);
        assert_eq!(msg.len(), a.buffered_amount(), "incorrect bufferedAmount");
    }

    pair.drive();

    let mut buf = vec![0u8; 3];
    let chunks = pair.server_stream(server_ch, si)?.read_sctp()?.unwrap();
    let result = chunks.read(&mut buf);
    assert!(result.is_err(), "expected error to be io.ErrShortBuffer");
    if let Err(err) = result {
        assert_eq!(
            Error::ErrShortBuffer,
            err,
            "expected error to be io.ErrShortBuffer"
        );
    }

    {
        let q = &pair
            .client_conn_mut(client_ch)
            .streams
            .get(&si)
            .unwrap()
            .reassembly_queue;
        assert!(!q.is_readable(), "should no longer be readable");
    }

    {
        let a = pair.client_conn_mut(client_ch);
        assert_eq!(0, a.buffered_amount(), "incorrect bufferedAmount");
    }

    close_association_pair(&mut pair, client_ch, server_ch, si);

    Ok(())
}

#[test]
fn test_assoc_unreliable_rexmit_ordered_no_fragment() -> Result<()> {
    //let _guard = subscribe();

    let si: u16 = 1;
    let mut sbuf = vec![0u8; 1000];
    for i in 0..sbuf.len() {
        sbuf[i] = (i & 0xff) as u8;
    }

    let (mut pair, client_ch, server_ch) = create_association_pair(AckMode::NoDelay, 0)?;

    establish_session_pair(&mut pair, client_ch, server_ch, si)?;

    // When we set the reliability value to 0 [times], then it will cause
    // the chunk to be abandoned immediately after the first transmission.
    pair.client_stream(client_ch, si)?
        .set_reliability_params(false, ReliabilityType::Rexmit, 0)?;
    pair.server_stream(server_ch, si)?
        .set_reliability_params(false, ReliabilityType::Rexmit, 0)?; // doesn't matter

    //br.drop_next_nwrites(0, 1).await; // drop the first packet (second one should be sacked)

    sbuf[0..4].copy_from_slice(&0u32.to_be_bytes());
    let n = pair.client_stream(client_ch, si)?.write_sctp(
        &Bytes::from(sbuf.clone()),
        PayloadProtocolIdentifier::Binary,
    )?;
    assert_eq!(sbuf.len(), n, "unexpected length of received data");
    pair.drive_client(); // send data to server
    pair.server.inbound.clear(); // Lose it
    debug!("dropping packet");

    sbuf[0..4].copy_from_slice(&1u32.to_be_bytes());
    let n = pair.client_stream(client_ch, si)?.write_sctp(
        &Bytes::from(sbuf.clone()),
        PayloadProtocolIdentifier::Binary,
    )?;
    assert_eq!(sbuf.len(), n, "unexpected length of received data");

    debug!("flush_buffers");
    pair.drive();

    let mut buf = vec![0u8; 2000];

    debug!("read_sctp");
    let chunks = pair.server_stream(server_ch, si)?.read_sctp()?.unwrap();
    let (n, ppi) = (chunks.len(), chunks.ppi);
    chunks.read(&mut buf)?;
    assert_eq!(n, sbuf.len(), "unexpected length of received data");
    assert_eq!(ppi, PayloadProtocolIdentifier::Binary, "unexpected ppi");
    assert_eq!(
        u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]),
        1,
        "unexpected received data"
    );

    debug!("process");
    pair.drive();

    {
        let q = &pair
            .client_conn_mut(client_ch)
            .streams
            .get(&si)
            .unwrap()
            .reassembly_queue;
        assert!(!q.is_readable(), "should no longer be readable");
    }

    close_association_pair(&mut pair, client_ch, server_ch, si);

    Ok(())
}

#[test]
fn test_assoc_unreliable_rexmit_ordered_fragment() -> Result<()> {
    //let _guard = subscribe();

    let si: u16 = 1;
    let mut sbuf = vec![0u8; 2000];
    for i in 0..sbuf.len() {
        sbuf[i] = (i & 0xff) as u8;
    }

    let (mut pair, client_ch, server_ch) = create_association_pair(AckMode::NoDelay, 0)?;

    establish_session_pair(&mut pair, client_ch, server_ch, si)?;

    {
        // lock RTO value at 100 [msec]
        let a = pair.client_conn_mut(client_ch);
        a.rto_mgr.set_rto(100, true);
    }
    // When we set the reliability value to 0 [times], then it will cause
    // the chunk to be abandoned immediately after the first transmission.
    pair.client_stream(client_ch, si)?
        .set_reliability_params(false, ReliabilityType::Rexmit, 0)?;
    pair.server_stream(server_ch, si)?
        .set_reliability_params(false, ReliabilityType::Rexmit, 0)?; // doesn't matter

    //br.drop_next_nwrites(0, 1).await; // drop the first packet (second one should be sacked)

    sbuf[0..4].copy_from_slice(&0u32.to_be_bytes());
    let n = pair.client_stream(client_ch, si)?.write_sctp(
        &Bytes::from(sbuf.clone()),
        PayloadProtocolIdentifier::Binary,
    )?;
    assert_eq!(sbuf.len(), n, "unexpected length of received data");
    pair.drive_client(); // send data to server
    pair.server.inbound.clear(); // Lose it

    sbuf[0..4].copy_from_slice(&1u32.to_be_bytes());
    let n = pair.client_stream(client_ch, si)?.write_sctp(
        &Bytes::from(sbuf.clone()),
        PayloadProtocolIdentifier::Binary,
    )?;
    assert_eq!(sbuf.len(), n, "unexpected length of received data");

    //log::debug!("flush_buffers");
    pair.drive();

    let mut buf = vec![0u8; 2000];

    //log::debug!("read_sctp");
    let chunks = pair.server_stream(server_ch, si)?.read_sctp()?.unwrap();
    let (n, ppi) = (chunks.len(), chunks.ppi);
    chunks.read(&mut buf)?;
    assert_eq!(n, sbuf.len(), "unexpected length of received data");
    assert_eq!(ppi, PayloadProtocolIdentifier::Binary, "unexpected ppi");
    assert_eq!(
        u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]),
        1,
        "unexpected received data"
    );

    //log::debug!("process");
    pair.drive();

    {
        let q = &pair
            .client_conn_mut(client_ch)
            .streams
            .get(&si)
            .unwrap()
            .reassembly_queue;
        assert!(!q.is_readable(), "should no longer be readable");
    }

    close_association_pair(&mut pair, client_ch, server_ch, si);

    Ok(())
}

#[test]
fn test_assoc_unreliable_rexmit_unordered_no_fragment() -> Result<()> {
    //let _guard = subscribe();

    let si: u16 = 2;
    let mut sbuf = vec![0u8; 1000];
    for i in 0..sbuf.len() {
        sbuf[i] = (i & 0xff) as u8;
    }

    let (mut pair, client_ch, server_ch) = create_association_pair(AckMode::NoDelay, 0)?;

    establish_session_pair(&mut pair, client_ch, server_ch, si)?;

    // When we set the reliability value to 0 [times], then it will cause
    // the chunk to be abandoned immediately after the first transmission.
    pair.client_stream(client_ch, si)?
        .set_reliability_params(true, ReliabilityType::Rexmit, 0)?;
    pair.server_stream(server_ch, si)?
        .set_reliability_params(true, ReliabilityType::Rexmit, 0)?; // doesn't matter

    //br.drop_next_nwrites(0, 1).await; // drop the first packet (second one should be sacked)

    sbuf[0..4].copy_from_slice(&0u32.to_be_bytes());
    let n = pair.client_stream(client_ch, si)?.write_sctp(
        &Bytes::from(sbuf.clone()),
        PayloadProtocolIdentifier::Binary,
    )?;
    assert_eq!(sbuf.len(), n, "unexpected length of received data");
    pair.drive_client(); // send data to server
    pair.server.inbound.clear(); // Lose it

    sbuf[0..4].copy_from_slice(&1u32.to_be_bytes());
    let n = pair.client_stream(client_ch, si)?.write_sctp(
        &Bytes::from(sbuf.clone()),
        PayloadProtocolIdentifier::Binary,
    )?;
    assert_eq!(sbuf.len(), n, "unexpected length of received data");

    //log::debug!("flush_buffers");
    pair.drive();

    let mut buf = vec![0u8; 2000];

    //log::debug!("read_sctp");
    let chunks = pair.server_stream(server_ch, si)?.read_sctp()?.unwrap();
    let (n, ppi) = (chunks.len(), chunks.ppi);
    chunks.read(&mut buf)?;
    assert_eq!(n, sbuf.len(), "unexpected length of received data");
    assert_eq!(ppi, PayloadProtocolIdentifier::Binary, "unexpected ppi");
    assert_eq!(
        u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]),
        1,
        "unexpected received data"
    );

    //log::debug!("process");
    pair.drive();

    {
        let q = &pair
            .client_conn_mut(client_ch)
            .streams
            .get(&si)
            .unwrap()
            .reassembly_queue;
        assert!(!q.is_readable(), "should no longer be readable");
    }

    close_association_pair(&mut pair, client_ch, server_ch, si);

    Ok(())
}

#[test]
fn test_assoc_unreliable_rexmit_unordered_fragment() -> Result<()> {
    //let _guard = subscribe();

    let si: u16 = 1;
    let mut sbuf = vec![0u8; 2000];
    for i in 0..sbuf.len() {
        sbuf[i] = (i & 0xff) as u8;
    }

    let (mut pair, client_ch, server_ch) = create_association_pair(AckMode::NoDelay, 0)?;

    establish_session_pair(&mut pair, client_ch, server_ch, si)?;

    // When we set the reliability value to 0 [times], then it will cause
    // the chunk to be abandoned immediately after the first transmission.
    pair.client_stream(client_ch, si)?
        .set_reliability_params(true, ReliabilityType::Rexmit, 0)?;
    pair.server_stream(server_ch, si)?
        .set_reliability_params(true, ReliabilityType::Rexmit, 0)?; // doesn't matter

    sbuf[0..4].copy_from_slice(&0u32.to_be_bytes());
    let n = pair.client_stream(client_ch, si)?.write_sctp(
        &Bytes::from(sbuf.clone()),
        PayloadProtocolIdentifier::Binary,
    )?;
    assert_eq!(sbuf.len(), n, "unexpected length of received data");
    pair.client.drive(pair.time, pair.server.addr);
    pair.client.outbound.clear();
    //debug!("outbound len={}", pair.client.outbound.len());

    sbuf[0..4].copy_from_slice(&1u32.to_be_bytes());
    let n = pair.client_stream(client_ch, si)?.write_sctp(
        &Bytes::from(sbuf.clone()),
        PayloadProtocolIdentifier::Binary,
    )?;
    assert_eq!(sbuf.len(), n, "unexpected length of received data");

    pair.drive();

    let mut buf = vec![0u8; 2000];

    //log::debug!("read_sctp");
    let chunks = pair.server_stream(server_ch, si)?.read_sctp()?.unwrap();
    let (n, ppi) = (chunks.len(), chunks.ppi);
    chunks.read(&mut buf)?;
    assert_eq!(n, sbuf.len(), "unexpected length of received data");
    assert_eq!(ppi, PayloadProtocolIdentifier::Binary, "unexpected ppi");
    assert_eq!(
        u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]),
        1,
        "unexpected received data"
    );

    //log::debug!("process");
    pair.drive();

    {
        let q = &pair
            .client_conn_mut(client_ch)
            .streams
            .get(&si)
            .unwrap()
            .reassembly_queue;
        assert!(!q.is_readable(), "should no longer be readable");
        assert_eq!(
            0,
            q.unordered.len(),
            "should be nothing in the unordered queue"
        );
        assert_eq!(
            0,
            q.unordered_chunks.len(),
            "should be nothing in the unorderedChunks list"
        );
    }

    close_association_pair(&mut pair, client_ch, server_ch, si);

    Ok(())
}

#[test]
fn test_assoc_unreliable_rexmit_timed_ordered() -> Result<()> {
    //let _guard = subscribe();

    let si: u16 = 3;
    let mut sbuf = vec![0u8; 1000];
    for i in 0..sbuf.len() {
        sbuf[i] = (i & 0xff) as u8;
    }

    let (mut pair, client_ch, server_ch) = create_association_pair(AckMode::NoDelay, 0)?;

    establish_session_pair(&mut pair, client_ch, server_ch, si)?;

    // When we set the reliability value to 0 [times], then it will cause
    // the chunk to be abandoned immediately after the first transmission.
    pair.client_stream(client_ch, si)?
        .set_reliability_params(false, ReliabilityType::Timed, 0)?;
    pair.server_stream(server_ch, si)?
        .set_reliability_params(false, ReliabilityType::Timed, 0)?; // doesn't matter

    //br.drop_next_nwrites(0, 1).await; // drop the first packet (second one should be sacked)

    sbuf[0..4].copy_from_slice(&0u32.to_be_bytes());
    let n = pair.client_stream(client_ch, si)?.write_sctp(
        &Bytes::from(sbuf.clone()),
        PayloadProtocolIdentifier::Binary,
    )?;
    assert_eq!(sbuf.len(), n, "unexpected length of received data");
    pair.client.drive(pair.time, pair.server.addr);
    pair.client.outbound.clear();

    sbuf[0..4].copy_from_slice(&1u32.to_be_bytes());
    let n = pair.client_stream(client_ch, si)?.write_sctp(
        &Bytes::from(sbuf.clone()),
        PayloadProtocolIdentifier::Binary,
    )?;
    assert_eq!(sbuf.len(), n, "unexpected length of received data");

    //log::debug!("flush_buffers");
    pair.drive();

    let mut buf = vec![0u8; 2000];

    //log::debug!("read_sctp");
    let chunks = pair.server_stream(server_ch, si)?.read_sctp()?.unwrap();
    let (n, ppi) = (chunks.len(), chunks.ppi);
    chunks.read(&mut buf)?;
    assert_eq!(n, sbuf.len(), "unexpected length of received data");
    assert_eq!(ppi, PayloadProtocolIdentifier::Binary, "unexpected ppi");
    assert_eq!(
        u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]),
        1,
        "unexpected received data"
    );

    //log::debug!("process");
    pair.drive();

    {
        let q = &pair
            .client_conn_mut(client_ch)
            .streams
            .get(&si)
            .unwrap()
            .reassembly_queue;
        assert!(!q.is_readable(), "should no longer be readable");
    }

    close_association_pair(&mut pair, client_ch, server_ch, si);

    Ok(())
}

#[test]
fn test_assoc_unreliable_rexmit_timed_unordered() -> Result<()> {
    //let _guard = subscribe();

    let si: u16 = 3;
    let mut sbuf = vec![0u8; 1000];
    for i in 0..sbuf.len() {
        sbuf[i] = (i & 0xff) as u8;
    }

    let (mut pair, client_ch, server_ch) = create_association_pair(AckMode::NoDelay, 0)?;

    establish_session_pair(&mut pair, client_ch, server_ch, si)?;

    // When we set the reliability value to 0 [times], then it will cause
    // the chunk to be abandoned immediately after the first transmission.
    pair.client_stream(client_ch, si)?
        .set_reliability_params(true, ReliabilityType::Timed, 0)?;
    pair.server_stream(server_ch, si)?
        .set_reliability_params(true, ReliabilityType::Timed, 0)?; // doesn't matter

    //br.drop_next_nwrites(0, 1).await; // drop the first packet (second one should be sacked)

    sbuf[0..4].copy_from_slice(&0u32.to_be_bytes());
    let n = pair.client_stream(client_ch, si)?.write_sctp(
        &Bytes::from(sbuf.clone()),
        PayloadProtocolIdentifier::Binary,
    )?;
    assert_eq!(sbuf.len(), n, "unexpected length of received data");
    pair.client.drive(pair.time, pair.server.addr);
    pair.client.outbound.clear();

    sbuf[0..4].copy_from_slice(&1u32.to_be_bytes());
    let n = pair.client_stream(client_ch, si)?.write_sctp(
        &Bytes::from(sbuf.clone()),
        PayloadProtocolIdentifier::Binary,
    )?;
    assert_eq!(sbuf.len(), n, "unexpected length of received data");

    //log::debug!("flush_buffers");
    pair.drive();

    let mut buf = vec![0u8; 2000];

    //log::debug!("read_sctp");
    let chunks = pair.server_stream(server_ch, si)?.read_sctp()?.unwrap();
    let (n, ppi) = (chunks.len(), chunks.ppi);
    chunks.read(&mut buf)?;
    assert_eq!(n, sbuf.len(), "unexpected length of received data");
    assert_eq!(ppi, PayloadProtocolIdentifier::Binary, "unexpected ppi");
    assert_eq!(
        u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]),
        1,
        "unexpected received data"
    );

    //log::debug!("process");
    pair.drive();

    {
        let q = &pair
            .client_conn_mut(client_ch)
            .streams
            .get(&si)
            .unwrap()
            .reassembly_queue;
        assert!(!q.is_readable(), "should no longer be readable");
        assert_eq!(
            0,
            q.unordered.len(),
            "should be nothing in the unordered queue"
        );
        assert_eq!(
            0,
            q.unordered_chunks.len(),
            "should be nothing in the unorderedChunks list"
        );
    }

    close_association_pair(&mut pair, client_ch, server_ch, si);

    Ok(())
}

//TODO: TestAssocT1InitTimer
//TODO: TestAssocT1CookieTimer
//TODO: TestAssocT3RtxTimer

/*FIXME
// 1) Send 4 packets. drop the first one.
// 2) Last 3 packet will be received, which triggers fast-retransmission
// 3) The first one is retransmitted, which makes s1 readable
// Above should be done before RTO occurs (fast recovery)
#[test]
fn test_assoc_congestion_control_fast_retransmission() -> Result<()> {
    let _guard = subscribe();

    let si: u16 = 6;
    let mut sbuf = vec![0u8; 1000];
    for i in 0..sbuf.len() {
        sbuf[i] = (i & 0xff) as u8;
    }

    let (mut pair, client_ch, server_ch) = create_association_pair(AckMode::Normal, 0)?;

    establish_session_pair(&mut pair, client_ch, server_ch, si)?;

    //br.drop_next_nwrites(0, 1).await; // drop the first packet (second one should be sacked)

    for i in 0..4u32 {
        sbuf[0..4].copy_from_slice(&i.to_be_bytes());
        let n = pair.client_stream(client_ch, si)?.write_sctp(
            &Bytes::from(sbuf.clone()),
            PayloadProtocolIdentifier::Binary,
        )?;
        assert_eq!(sbuf.len(), n, "unexpected length of received data");
        pair.client.drive(pair.time, pair.server.addr);
        if i == 0 {
            //drop the first packet
            pair.client.outbound.clear();
        }
    }

    // process packets for 500 msec, assuming that the fast retrans/recover
    // should complete within 500 msec.
    /*for _ in 0..50 {
        br.tick().await;
        tokio::time::sleep(Duration::from_millis(10)).await;
    }*/
    debug!("advance 500ms");
    pair.time += Duration::from_millis(500);
    pair.step();

    let mut buf = vec![0u8; 3000];

    // Try to read all 4 packets
    for i in 0..4 {
        {
            let q = &pair
                .server_conn_mut(server_ch)
                .streams
                .get(&si)
                .unwrap()
                .reassembly_queue;
            assert!(q.is_readable(), "should be readable at {}", i);
        }

        let chunks = pair.server_stream(server_ch, si)?.read_sctp()?.unwrap();
        let (n, ppi) = (chunks.len(), chunks.ppi);
        chunks.read(&mut buf)?;
        assert_eq!(n, sbuf.len(), "unexpected length of received data");
        assert_eq!(ppi, PayloadProtocolIdentifier::Binary, "unexpected ppi");
        assert_eq!(
            u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]),
            i,
            "unexpected received data"
        );
    }

    pair.drive();
    //br.process().await;

    {
        let a = pair.client_conn_mut(client_ch);
        assert!(!a.in_fast_recovery, "should not be in fast-recovery");
        debug!("nSACKs      : {}", a.stats.get_num_sacks());
        debug!("nFastRetrans: {}", a.stats.get_num_fast_retrans());

        assert_eq!(1, a.stats.get_num_fast_retrans(), "should be 1");
    }
    {
        let b = pair.server_conn_mut(server_ch);
        debug!("nDATAs      : {}", b.stats.get_num_datas());
        debug!("nAckTimeouts: {}", b.stats.get_num_ack_timeouts());
    }

    close_association_pair(&mut pair, client_ch, server_ch, si);

    Ok(())
}*/

#[test]
fn test_assoc_congestion_control_congestion_avoidance() -> Result<()> {
    //let _guard = subscribe();

    let max_receive_buffer_size: u32 = 64 * 1024;
    let si: u16 = 6;
    let n_packets_to_send: u32 = 2000;

    let mut sbuf = vec![0u8; 1000];
    for i in 0..sbuf.len() {
        sbuf[i] = (i & 0xff) as u8;
    }

    let (mut pair, client_ch, server_ch) =
        create_association_pair(AckMode::Normal, max_receive_buffer_size)?;

    establish_session_pair(&mut pair, client_ch, server_ch, si)?;

    {
        pair.client_conn_mut(client_ch).stats.reset();
        pair.server_conn_mut(server_ch).stats.reset();
    }

    for i in 0..n_packets_to_send {
        sbuf[0..4].copy_from_slice(&i.to_be_bytes());
        let n = pair.client_stream(client_ch, si)?.write_sctp(
            &Bytes::from(sbuf.clone()),
            PayloadProtocolIdentifier::Binary,
        )?;
        assert_eq!(sbuf.len(), n, "unexpected length of received data");
    }
    pair.drive_client();
    //debug!("pair.drive_client() done");

    let mut rbuf = vec![0u8; 3000];

    // Repeat calling br.Tick() until the buffered amount becomes 0
    let mut n_packets_received = 0u32;
    while pair.client_conn_mut(client_ch).buffered_amount() > 0
        && n_packets_received < n_packets_to_send
    {
        /*println!("timestamp: {:?}", pair.time);
        println!(
            "buffered_amount {}, pair.server.inbound {}, n_packets_received {}, n_packets_to_send {}",
            pair.client_conn_mut(client_ch).buffered_amount(),
            pair.server.inbound.len(),
            n_packets_received,
            n_packets_to_send
        );*/

        pair.step();

        while let Some(chunks) = pair.server_stream(server_ch, si)?.read_sctp()? {
            let (n, ppi) = (chunks.len(), chunks.ppi);
            chunks.read(&mut rbuf)?;
            assert_eq!(sbuf.len(), n, "unexpected length of received data");
            assert_eq!(
                n_packets_received,
                u32::from_be_bytes([rbuf[0], rbuf[1], rbuf[2], rbuf[3]]),
                "unexpected length of received data"
            );
            assert_eq!(ppi, PayloadProtocolIdentifier::Binary, "unexpected ppi");

            n_packets_received += 1;
        }

        //pair.drive_client();
    }

    pair.drive();
    //println!("timestamp: {:?}", pair.time);

    assert_eq!(
        n_packets_received, n_packets_to_send,
        "unexpected num of packets received"
    );

    {
        let a = pair.client_conn_mut(client_ch);

        assert!(!a.in_fast_recovery, "should not be in fast-recovery");
        assert!(
            a.cwnd > a.ssthresh,
            "should be in congestion avoidance mode"
        );
        assert!(
            a.ssthresh >= max_receive_buffer_size,
            "{} should not be less than the initial size of 128KB {}",
            a.ssthresh,
            max_receive_buffer_size
        );

        debug!("nSACKs      : {}", a.stats.get_num_sacks());
        debug!("nT3Timeouts: {}", a.stats.get_num_t3timeouts());

        assert!(
            a.stats.get_num_sacks() <= n_packets_to_send as u64 / 2,
            "too many sacks"
        );
        assert_eq!(0, a.stats.get_num_t3timeouts(), "should be no retransmit");
    }
    {
        assert_eq!(
            0,
            pair.server_conn_mut(server_ch)
                .streams
                .get(&si)
                .unwrap()
                .get_num_bytes_in_reassembly_queue(),
            "reassembly queue should be empty"
        );

        let b = pair.server_conn_mut(server_ch);

        debug!("nDATAs      : {}", b.stats.get_num_datas());

        assert_eq!(
            n_packets_to_send as u64,
            b.stats.get_num_datas(),
            "packet count mismatch"
        );
    }

    close_association_pair(&mut pair, client_ch, server_ch, si);

    Ok(())
}

#[test]
fn test_assoc_congestion_control_slow_reader() -> Result<()> {
    //let _guard = subscribe();

    let max_receive_buffer_size: u32 = 64 * 1024;
    let si: u16 = 6;
    let n_packets_to_send: u32 = 130;

    let mut sbuf = vec![0u8; 1000];
    for i in 0..sbuf.len() {
        sbuf[i] = (i & 0xff) as u8;
    }

    let (mut pair, client_ch, server_ch) =
        create_association_pair(AckMode::Normal, max_receive_buffer_size)?;

    establish_session_pair(&mut pair, client_ch, server_ch, si)?;

    for i in 0..n_packets_to_send {
        sbuf[0..4].copy_from_slice(&i.to_be_bytes());
        let n = pair.client_stream(client_ch, si)?.write_sctp(
            &Bytes::from(sbuf.clone()),
            PayloadProtocolIdentifier::Binary,
        )?;
        assert_eq!(sbuf.len(), n, "unexpected length of received data");
    }
    pair.drive_client();

    let mut rbuf = vec![0u8; 3000];

    // 1. First forward packets to receiver until rwnd becomes 0
    // 2. Wait until the sender's cwnd becomes 1*MTU (RTO occurred)
    // 3. Stat reading a1's data
    let mut n_packets_received = 0u32;
    let mut has_rtoed = false;
    while pair.client_conn_mut(client_ch).buffered_amount() > 0
        && n_packets_received < n_packets_to_send
    {
        /*println!(
            "buffered_amount {}, pair.server.inbound {}, n_packets_received {}, n_packets_to_send {}",
            pair.client_conn_mut(client_ch).buffered_amount(),
            pair.server.inbound.len(),
            n_packets_received,
            n_packets_to_send
        );*/

        if !has_rtoed {
            let rwnd = pair
                .server_conn_mut(server_ch)
                .get_my_receiver_window_credit();
            let cwnd = pair.client_conn_mut(client_ch).cwnd;
            let cmtu = pair.client_conn_mut(client_ch).mtu;
            if cwnd > cmtu || rwnd > 0 {
                // Do not read until a1.getMyReceiverWindowCredit() becomes zero
                pair.step();
                continue;
            }

            has_rtoed = true;
        }

        while let Some(chunks) = pair.server_stream(server_ch, si)?.read_sctp()? {
            let (n, ppi) = (chunks.len(), chunks.ppi);
            chunks.read(&mut rbuf)?;
            assert_eq!(sbuf.len(), n, "unexpected length of received data");
            assert_eq!(
                n_packets_received,
                u32::from_be_bytes([rbuf[0], rbuf[1], rbuf[2], rbuf[3]]),
                "unexpected length of received data"
            );
            assert_eq!(ppi, PayloadProtocolIdentifier::Binary, "unexpected ppi");

            n_packets_received += 1;
        }

        pair.step();
    }

    pair.drive();

    assert_eq!(
        n_packets_received, n_packets_to_send,
        "unexpected num of packets received"
    );
    assert_eq!(
        0,
        pair.server_conn_mut(server_ch)
            .streams
            .get(&si)
            .unwrap()
            .get_num_bytes_in_reassembly_queue(),
        "reassembly queue should be empty"
    );

    {
        let a = pair.client_conn_mut(client_ch);
        debug!("nSACKs      : {}", a.stats.get_num_sacks());
    }
    {
        let b = pair.server_conn_mut(server_ch);
        debug!("nDATAs      : {}", b.stats.get_num_datas());
        debug!("nAckTimeouts: {}", b.stats.get_num_ack_timeouts());
    }

    close_association_pair(&mut pair, client_ch, server_ch, si);

    Ok(())
}

#[test]
fn test_assoc_delayed_ack() -> Result<()> {
    //let _guard = subscribe();

    let si: u16 = 6;
    let mut sbuf = vec![0u8; 1000];
    let mut rbuf = vec![0u8; 1500];
    for i in 0..sbuf.len() {
        sbuf[i] = (i & 0xff) as u8;
    }

    let (mut pair, client_ch, server_ch) = create_association_pair(AckMode::AlwaysDelay, 0)?;

    establish_session_pair(&mut pair, client_ch, server_ch, si)?;

    {
        pair.client_conn_mut(client_ch).stats.reset();
        pair.server_conn_mut(server_ch).stats.reset();
    }

    let n = pair.client_stream(client_ch, si)?.write_sctp(
        &Bytes::from(sbuf.clone()),
        PayloadProtocolIdentifier::Binary,
    )?;
    assert_eq!(sbuf.len(), n, "unexpected length of received data");
    pair.drive_client();

    // Repeat calling br.Tick() until the buffered amount becomes 0
    let since = pair.time;
    let mut n_packets_received = 0;
    while pair.client_conn_mut(client_ch).buffered_amount() > 0 {
        pair.step();

        while let Some(chunks) = pair.server_stream(server_ch, si)?.read_sctp()? {
            let (n, ppi) = (chunks.len(), chunks.ppi);
            chunks.read(&mut rbuf)?;
            assert_eq!(sbuf.len(), n, "unexpected length of received data");
            assert_eq!(ppi, PayloadProtocolIdentifier::Binary, "unexpected ppi");

            n_packets_received += 1;
        }
    }
    let delay = (pair.time.duration_since(since).as_millis() as f64) / 1000.0;
    debug!("received in {} seconds", delay);
    assert!(delay >= 0.2, "should be >= 200msec");

    pair.drive();

    assert_eq!(n_packets_received, 1, "unexpected num of packets received");
    assert_eq!(
        0,
        pair.server_conn_mut(server_ch)
            .streams
            .get(&si)
            .unwrap()
            .get_num_bytes_in_reassembly_queue(),
        "reassembly queue should be empty"
    );

    let a_num_sacks = {
        let a = pair.client_conn_mut(client_ch);
        debug!("nSACKs      : {}", a.stats.get_num_sacks());
        assert_eq!(0, a.stats.get_num_t3timeouts(), "should be no retransmit");
        a.stats.get_num_sacks()
    };

    {
        let b = pair.server_conn_mut(server_ch);

        debug!("nDATAs      : {}", b.stats.get_num_datas());
        debug!("nAckTimeouts: {}", b.stats.get_num_ack_timeouts());

        assert_eq!(1, b.stats.get_num_datas(), "DATA chunk count mismatch");
        assert_eq!(
            a_num_sacks,
            b.stats.get_num_datas(),
            "sack count should be equal to the number of data chunks"
        );
        assert_eq!(
            1,
            b.stats.get_num_ack_timeouts(),
            "ackTimeout count mismatch"
        );
    }

    close_association_pair(&mut pair, client_ch, server_ch, si);

    Ok(())
}

#[test]
fn test_assoc_reset_close_one_way() -> Result<()> {
    //let _guard = subscribe();

    let si: u16 = 1;
    let msg: Bytes = Bytes::from_static(b"ABC");

    let (mut pair, client_ch, server_ch) = create_association_pair(AckMode::NoDelay, 0)?;

    establish_session_pair(&mut pair, client_ch, server_ch, si)?;

    {
        let a = pair.client_conn_mut(client_ch);
        assert_eq!(0, a.buffered_amount(), "incorrect bufferedAmount");
    }

    let n = pair
        .client_stream(client_ch, si)?
        .write_sctp(&msg, PayloadProtocolIdentifier::Binary)?;
    assert_eq!(msg.len(), n, "unexpected length of received data");
    {
        let a = pair.client_conn_mut(client_ch);
        assert_eq!(msg.len(), a.buffered_amount(), "incorrect bufferedAmount");
    }
    pair.step();

    let mut buf = vec![0u8; 32];

    while pair.server_stream(server_ch, si).is_ok() {
        debug!("s1.read_sctp begin");
        match pair.server_stream(server_ch, si)?.read_sctp() {
            Ok(chunks_opt) => {
                if let Some(chunks) = chunks_opt {
                    let (n, ppi) = (chunks.len(), chunks.ppi);
                    chunks.read(&mut buf)?;
                    debug!("s1.read_sctp done with {:?}", &buf[..n]);
                    assert_eq!(ppi, PayloadProtocolIdentifier::Binary, "unexpected ppi");
                    assert_eq!(n, msg.len(), "unexpected length of received data");
                }

                debug!("s0.close");
                pair.client_stream(client_ch, si)?.stop()?; // send reset

                pair.step();
            }
            Err(err) => {
                debug!("s1.read_sctp err {:?}", err);
                break;
            }
        }
    }

    pair.drive();

    close_association_pair(&mut pair, client_ch, server_ch, si);

    Ok(())
}

#[test]
fn test_assoc_reset_close_both_ways() -> Result<()> {
    //let _guard = subscribe();

    let si: u16 = 1;
    let msg: Bytes = Bytes::from_static(b"ABC");

    let (mut pair, client_ch, server_ch) = create_association_pair(AckMode::NoDelay, 0)?;

    establish_session_pair(&mut pair, client_ch, server_ch, si)?;

    {
        let a = pair.client_conn_mut(client_ch);
        assert_eq!(0, a.buffered_amount(), "incorrect bufferedAmount");
    }

    let n = pair
        .client_stream(client_ch, si)?
        .write_sctp(&msg, PayloadProtocolIdentifier::Binary)?;
    assert_eq!(msg.len(), n, "unexpected length of received data");
    {
        let a = pair.client_conn_mut(client_ch);
        assert_eq!(msg.len(), a.buffered_amount(), "incorrect bufferedAmount");
    }
    pair.step();

    let mut buf = vec![0u8; 32];

    while pair.server_stream(server_ch, si).is_ok() || pair.client_stream(client_ch, si).is_ok() {
        if pair.server_stream(server_ch, si).is_ok() {
            debug!("s1.read_sctp begin");
            match pair.server_stream(server_ch, si)?.read_sctp() {
                Ok(chunks_opt) => {
                    if let Some(chunks) = chunks_opt {
                        let (n, ppi) = (chunks.len(), chunks.ppi);
                        chunks.read(&mut buf)?;
                        debug!("s1.read_sctp done with {:?}", &buf[..n]);
                        assert_eq!(ppi, PayloadProtocolIdentifier::Binary, "unexpected ppi");
                        assert_eq!(n, msg.len(), "unexpected length of received data");
                    }
                }
                Err(err) => {
                    debug!("s1.read_sctp err {:?}", err);
                    break;
                }
            }
        }

        if pair.client_stream(client_ch, si).is_ok() {
            debug!("s0.read_sctp begin");
            match pair.client_stream(client_ch, si)?.read_sctp() {
                Ok(chunks_opt) => {
                    if let Some(chunks) = chunks_opt {
                        let (n, ppi) = (chunks.len(), chunks.ppi);
                        chunks.read(&mut buf)?;
                        debug!("s0.read_sctp done with {:?}", &buf[..n]);
                        assert_eq!(ppi, PayloadProtocolIdentifier::Binary, "unexpected ppi");
                        assert_eq!(n, msg.len(), "unexpected length of received data");
                    }
                }
                Err(err) => {
                    debug!("s0.read_sctp err {:?}", err);
                    break;
                }
            }
        }

        if pair.client_stream(client_ch, si).is_ok() {
            pair.client_stream(client_ch, si)?.stop()?; // send reset
        }
        if pair.server_stream(server_ch, si).is_ok() {
            pair.server_stream(server_ch, si)?.stop()?; // send reset
        }

        pair.step();
    }

    pair.drive();

    close_association_pair(&mut pair, client_ch, server_ch, si);

    Ok(())
}

#[test]
fn test_assoc_abort() -> Result<()> {
    //let _guard = subscribe();

    let si: u16 = 1;

    let (mut pair, client_ch, server_ch) = create_association_pair(AckMode::NoDelay, 0)?;

    establish_session_pair(&mut pair, client_ch, server_ch, si)?;

    let transmit = {
        let abort = ChunkAbort {
            error_causes: vec![ErrorCauseProtocolViolation {
                code: PROTOCOL_VIOLATION,
                ..Default::default()
            }],
        };

        let packet = pair
            .client_conn_mut(client_ch)
            .create_packet(vec![Box::new(abort)])
            .marshal()?;

        Transmit {
            now: pair.time,
            remote: pair.server.addr,
            ecn: None,
            local_ip: None,
            payload: Payload::RawEncode(vec![packet]),
        }
    };

    // Both associations are established
    assert_eq!(
        AssociationState::Established,
        pair.client_conn_mut(client_ch).state()
    );
    assert_eq!(
        AssociationState::Established,
        pair.server_conn_mut(server_ch).state()
    );

    debug!("send ChunkAbort");
    pair.client.outbound.push_back(transmit);

    pair.drive();

    // The receiving association should be closed because it got an ABORT
    assert_eq!(
        AssociationState::Established,
        pair.client_conn_mut(client_ch).state()
    );
    assert_eq!(
        AssociationState::Closed,
        pair.server_conn_mut(server_ch).state()
    );

    close_association_pair(&mut pair, client_ch, server_ch, si);

    Ok(())
}

#[test]
fn test_association_handle_packet_before_init() -> Result<()> {
    //let _guard = subscribe();

    let tests = vec![
        (
            "InitAck",
            Packet {
                common_header: CommonHeader {
                    source_port: 1,
                    destination_port: 1,
                    verification_tag: 0,
                },
                chunks: vec![Box::new(ChunkInit {
                    is_ack: true,
                    initiate_tag: 1,
                    num_inbound_streams: 1,
                    num_outbound_streams: 1,
                    advertised_receiver_window_credit: 1500,
                    ..Default::default()
                })],
            },
        ),
        (
            "Abort",
            Packet {
                common_header: CommonHeader {
                    source_port: 1,
                    destination_port: 1,
                    verification_tag: 0,
                },
                chunks: vec![Box::new(ChunkAbort::default())],
            },
        ),
        (
            "CoockeEcho",
            Packet {
                common_header: CommonHeader {
                    source_port: 1,
                    destination_port: 1,
                    verification_tag: 0,
                },
                chunks: vec![Box::new(ChunkCookieEcho::default())],
            },
        ),
        (
            "HeartBeat",
            Packet {
                common_header: CommonHeader {
                    source_port: 1,
                    destination_port: 1,
                    verification_tag: 0,
                },
                chunks: vec![Box::new(ChunkHeartbeat::default())],
            },
        ),
        (
            "PayloadData",
            Packet {
                common_header: CommonHeader {
                    source_port: 1,
                    destination_port: 1,
                    verification_tag: 0,
                },
                chunks: vec![Box::new(ChunkPayloadData::default())],
            },
        ),
        (
            "Sack",
            Packet {
                common_header: CommonHeader {
                    source_port: 1,
                    destination_port: 1,
                    verification_tag: 0,
                },
                chunks: vec![Box::new(ChunkSelectiveAck {
                    cumulative_tsn_ack: 1000,
                    advertised_receiver_window_credit: 1500,
                    gap_ack_blocks: vec![GapAckBlock {
                        start: 100,
                        end: 200,
                    }],
                    ..Default::default()
                })],
            },
        ),
        (
            "Reconfig",
            Packet {
                common_header: CommonHeader {
                    source_port: 1,
                    destination_port: 1,
                    verification_tag: 0,
                },
                chunks: vec![Box::new(ChunkReconfig {
                    param_a: Some(Box::new(ParamOutgoingResetRequest::default())),
                    param_b: Some(Box::new(ParamReconfigResponse::default())),
                })],
            },
        ),
        (
            "ForwardTSN",
            Packet {
                common_header: CommonHeader {
                    source_port: 1,
                    destination_port: 1,
                    verification_tag: 0,
                },
                chunks: vec![Box::new(ChunkForwardTsn {
                    new_cumulative_tsn: 100,
                    ..Default::default()
                })],
            },
        ),
        (
            "Error",
            Packet {
                common_header: CommonHeader {
                    source_port: 1,
                    destination_port: 1,
                    verification_tag: 0,
                },
                chunks: vec![Box::new(ChunkError::default())],
            },
        ),
        (
            "Shutdown",
            Packet {
                common_header: CommonHeader {
                    source_port: 1,
                    destination_port: 1,
                    verification_tag: 0,
                },
                chunks: vec![Box::new(ChunkShutdown::default())],
            },
        ),
        (
            "ShutdownAck",
            Packet {
                common_header: CommonHeader {
                    source_port: 1,
                    destination_port: 1,
                    verification_tag: 0,
                },
                chunks: vec![Box::new(ChunkShutdownAck::default())],
            },
        ),
        (
            "ShutdownComplete",
            Packet {
                common_header: CommonHeader {
                    source_port: 1,
                    destination_port: 1,
                    verification_tag: 0,
                },
                chunks: vec![Box::new(ChunkShutdownComplete::default())],
            },
        ),
    ];

    let remote = SocketAddr::from_str("0.0.0.0:0").unwrap();

    for (name, packet) in tests {
        debug!("testing {}", name);

        //let (a_conn, charlie_conn) = pipe();
        let config = Arc::new(TransportConfig::default());
        let mut a = Association::new(None, config, 0, remote, None, Instant::now());

        let packet = packet.marshal()?;
        a.handle_event(AssociationEvent(AssociationEventInner::Datagram(
            Transmit {
                now: Instant::now(),
                remote,
                ecn: None,
                local_ip: None,
                payload: Payload::RawEncode(vec![packet]),
            },
        )));

        a.close()?;
    }

    Ok(())
}

/*
TODO: The following tests will be moved to sctp-async tests:
struct FakeEchoConn {
    wr_tx: Mutex<mpsc::Sender<Vec<u8>>>,
    rd_rx: Mutex<mpsc::Receiver<Vec<u8>>>,
    bytes_sent: AtomicUsize,
    bytes_received: AtomicUsize,
}

impl FakeEchoConn {
    fn new() -> impl Conn + AsAny {
        let (wr_tx, rd_rx) = mpsc::channel(1);
        FakeEchoConn {
            wr_tx: Mutex::new(wr_tx),
            rd_rx: Mutex::new(rd_rx),
            bytes_sent: AtomicUsize::new(0),
            bytes_received: AtomicUsize::new(0),
        }
    }
}

trait AsAny {
    fn as_any(&self) -> &(dyn std::any::Any + Send + Sync);
}

impl AsAny for FakeEchoConn {
    fn as_any(&self) -> &(dyn std::any::Any + Send + Sync) {
        self
    }
}

type UResult<T> = std::result::Result<T, util::Error>;

#[async_trait]
impl Conn for FakeEchoConn {
    fn connect(&self, _addr: SocketAddr) -> UResult<()> {
        Err(io::Error::new(io::ErrorKind::Other, "Not applicable").into())
    }

    fn recv(&self, b: &mut [u8]) -> UResult<usize> {
        let mut rd_rx = self.rd_rx.lock().await;
        let v = match rd_rx.recv().await {
            Some(v) => v,
            None => {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "Unexpected EOF").into())
            }
        };
        let l = std::cmp::min(v.len(), b.len());
        b[..l].copy_from_slice(&v[..l]);
        self.bytes_received.fetch_add(l, Ordering::SeqCst);
        Ok(l)
    }

    fn recv_from(&self, _buf: &mut [u8]) -> UResult<(usize, SocketAddr)> {
        Err(io::Error::new(io::ErrorKind::Other, "Not applicable").into())
    }

    fn send(&self, b: &[u8]) -> UResult<usize> {
        let wr_tx = self.wr_tx.lock().await;
        match wr_tx.send(b.to_vec()).await {
            Ok(_) => {}
            Err(err) => return Err(io::Error::new(io::ErrorKind::Other, err.to_string()).into()),
        };
        self.bytes_sent.fetch_add(b.len(), Ordering::SeqCst);
        Ok(b.len())
    }

    fn send_to(&self, _buf: &[u8], _target: SocketAddr) -> UResult<usize> {
        Err(io::Error::new(io::ErrorKind::Other, "Not applicable").into())
    }

    fn local_addr(&self) -> UResult<SocketAddr> {
        Err(io::Error::new(io::ErrorKind::AddrNotAvailable, "Addr Not Available").into())
    }

    fn remote_addr(&self) -> Option<SocketAddr> {
        None
    }

    fn close(&self) -> UResult<()> {
        Ok(())
    }
}

//use std::io::Write;

#[test]
fn test_stats() -> Result<()> {
    /*env_logger::Builder::new()
    .format(|buf, record| {
        writeln!(
            buf,
            "{}:{} [{}] {} - {}",
            record.file().unwrap_or("unknown"),
            record.line().unwrap_or(0),
            record.level(),
            chrono::Local::now().format("%H:%M:%S.%6f"),
            record.args()
        )
    })
    .filter(None, log::LevelFilter::Trace)
    .init();*/

    let conn = Arc::new(FakeEchoConn::new());
    let a = Association::client(Config {
        net_conn: Arc::clone(&conn) as Arc<dyn Conn + Send + Sync>,
        max_receive_buffer_size: 0,
        max_message_size: 0,
        name: "client".to_owned(),
    })
    .await?;

    if let Some(conn) = conn.as_any().downcast_ref::<FakeEchoConn>() {
        assert_eq!(
            conn.bytes_received.load(Ordering::SeqCst),
            a.bytes_received()
        );
        assert_eq!(conn.bytes_sent.load(Ordering::SeqCst), a.bytes_sent());
    } else {
        assert!(false, "must be FakeEchoConn");
    }

    Ok(())
}

fn create_assocs() -> Result<(Association, Association)> {
    let addr1 = SocketAddr::from_str("0.0.0.0:0").unwrap();
    let addr2 = SocketAddr::from_str("0.0.0.0:0").unwrap();

    let udp1 = UdpSocket::bind(addr1).await.unwrap();
    let udp2 = UdpSocket::bind(addr2).await.unwrap();

    udp1.connect(udp2.local_addr().unwrap()).await.unwrap();
    udp2.connect(udp1.local_addr().unwrap()).await.unwrap();

    let (a1chan_tx, mut a1chan_rx) = mpsc::channel(1);
    let (a2chan_tx, mut a2chan_rx) = mpsc::channel(1);

    tokio::spawn(async move {
        let a = Association::client(Config {
            net_conn: Arc::new(udp1),
            max_receive_buffer_size: 0,
            max_message_size: 0,
            name: "client".to_owned(),
        })
        .await?;

        let _ = a1chan_tx.send(a).await;

        Result::<()>::Ok(())
    });

    tokio::spawn(async move {
        let a = Association::server(Config {
            net_conn: Arc::new(udp2),
            max_receive_buffer_size: 0,
            max_message_size: 0,
            name: "server".to_owned(),
        })
        .await?;

        let _ = a2chan_tx.send(a).await;

        Result::<()>::Ok(())
    });

    let timer1 = tokio::time::sleep(Duration::from_secs(1));
    tokio::pin!(timer1);
    let a1 = tokio::select! {
        _ = timer1.as_mut() =>{
            assert!(false,"timed out waiting for a1");
            return Err(Error::Other("timed out waiting for a1".to_owned()).into());
        },
        a1 = a1chan_rx.recv() => {
            a1.unwrap()
        }
    };

    let timer2 = tokio::time::sleep(Duration::from_secs(1));
    tokio::pin!(timer2);
    let a2 = tokio::select! {
        _ = timer2.as_mut() =>{
            assert!(false,"timed out waiting for a2");
            return Err(Error::Other("timed out waiting for a2".to_owned()).into());
        },
        a2 = a2chan_rx.recv() => {
            a2.unwrap()
        }
    };

    Ok((a1, a2))
}

//use std::io::Write;
//TODO: remove this conditional test
#[cfg(not(target_os = "windows"))]
#[test]
fn test_association_shutdown() -> Result<()> {
    /*env_logger::Builder::new()
    .format(|buf, record| {
        writeln!(
            buf,
            "{}:{} [{}] {} - {}",
            record.file().unwrap_or("unknown"),
            record.line().unwrap_or(0),
            record.level(),
            chrono::Local::now().format("%H:%M:%S.%6f"),
            record.args()
        )
    })
    .filter(None, log::LevelFilter::Trace)
    .init();*/

    let (a1, a2) = create_assocs().await?;

    let s11 = a1.open_stream(1, PayloadProtocolIdentifier::String).await?;
    let s21 = a2.open_stream(1, PayloadProtocolIdentifier::String).await?;

    let test_data = Bytes::from_static(b"test");

    let n = s11.write(&test_data).await?;
    assert_eq!(test_data.len(), n);

    let mut buf = vec![0u8; test_data.len()];
    let n = s21.read(&mut buf).await?;
    assert_eq!(test_data.len(), n);
    assert_eq!(&test_data, &buf[0..n]);

    if let Ok(result) = tokio::time::timeout(Duration::from_secs(1), a1.shutdown()).await {
        assert!(result.is_ok(), "shutdown should be ok");
    } else {
        assert!(false, "shutdown timeout");
    }

    {
        let mut close_loop_ch_rx = a2.close_loop_ch_rx.lock().await;

        // Wait for close read loop channels to prevent flaky tests.
        let timer2 = tokio::time::sleep(Duration::from_secs(1));
        tokio::pin!(timer2);
        tokio::select! {
            _ = timer2.as_mut() =>{
                assert!(false,"timed out waiting for a2 read loop to close");
            },
            _ = close_loop_ch_rx.recv() => {
                log::debug!("recv a2.close_loop_ch_rx");
            }
        };
    }
    Ok(())
}


fn test_association_shutdown_during_write() -> Result<()> {
    //let _guard = subscribe();

    let (a1, a2) = create_assocs().await?;

    let s11 = a1.open_stream(1, PayloadProtocolIdentifier::String).await?;
    let s21 = a2.open_stream(1, PayloadProtocolIdentifier::String).await?;

    let (writing_done_tx, mut writing_done_rx) = mpsc::channel::<()>(1);
    let ss21 = Arc::clone(&s21);
    tokio::spawn(async move {
        let mut i = 0;
        while ss21.write(&Bytes::from(vec![i])).await.is_ok() {
            if i == 255 {
                i = 0;
            } else {
                i += 1;
            }

            if i % 100 == 0 {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }

        drop(writing_done_tx);
    });

    let test_data = Bytes::from_static(b"test");

    let n = s11.write(&test_data).await?;
    assert_eq!(test_data.len(), n);

    let mut buf = vec![0u8; test_data.len()];
    let n = s21.read(&mut buf).await?;
    assert_eq!(test_data.len(), n);
    assert_eq!(&test_data, &buf[0..n]);

    {
        let mut close_loop_ch_rx = a1.close_loop_ch_rx.lock().await;
        tokio::select! {
            res = tokio::time::timeout(Duration::from_secs(1), a1.shutdown()) => {
                if let Ok(result) = res {
                    assert!(result.is_ok(), "shutdown should be ok");
                } else {
                    assert!(false, "shutdown timeout");
                }
            }
            _ = writing_done_rx.recv() => {
                log::debug!("writing_done_rx");
                let result = close_loop_ch_rx.recv().await;
                log::debug!("a1.close_loop_ch_rx.recv: {:?}", result);
            },
        };
    }

    {
        let mut close_loop_ch_rx = a2.close_loop_ch_rx.lock().await;
        // Wait for close read loop channels to prevent flaky tests.
        let timer2 = tokio::time::sleep(Duration::from_secs(1));
        tokio::pin!(timer2);
        tokio::select! {
            _ = timer2.as_mut() =>{
                assert!(false,"timed out waiting for a2 read loop to close");
            },
            _ = close_loop_ch_rx.recv() => {
                log::debug!("recv a2.close_loop_ch_rx");
            }
        };
    }

    Ok(())
}*/
