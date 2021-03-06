use super::*;
use crate::util::*;

use async_trait::async_trait;
use crc32fast::Hasher;
use std::fmt;
use std::ops::Add;
use std::sync::atomic::{AtomicU16, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::broadcast;

#[derive(Default)]
pub struct CandidateBaseConfig {
    pub candidate_id: String,
    pub network: String,
    pub address: String,
    pub port: u16,
    pub component: u16,
    pub priority: u32,
    pub foundation: String,
    pub conn: Option<Arc<dyn util::Conn + Send + Sync>>,
    pub initialized_ch: Option<broadcast::Receiver<()>>,
}

pub(crate) type OnClose = fn() -> Result<(), Error>;

#[derive(Clone)]
pub struct CandidateBase {
    pub(crate) id: String,
    pub(crate) network_type: Arc<AtomicU8>,
    pub(crate) candidate_type: CandidateType,

    pub(crate) component: Arc<AtomicU16>,
    pub(crate) address: String,
    pub(crate) port: u16,
    pub(crate) related_address: Option<CandidateRelatedAddress>,
    pub(crate) tcp_type: TCPType,

    pub(crate) resolved_addr: SocketAddr,

    pub(crate) last_sent: Arc<AtomicU64>,
    pub(crate) last_received: Arc<AtomicU64>,
    pub(crate) conn: Option<Arc<dyn util::Conn + Send + Sync>>,

    //TODO:pub(crate) currAgent :Option<Agent>,
    //TODO:pub(crate) closeCh   chan struct{}
    //TODO:pub(crate) closedCh  chan struct{}
    pub(crate) foundation_override: String,
    pub(crate) priority_override: u32,

    //CandidateHost
    pub(crate) network: String,
    //CandidateRelay
    pub(crate) relay_client: Option<Arc<turn::client::Client>>,
}

/* TODO:
// Done implements context.Context
func (c *candidateBase) Done() <-chan struct{} {
    return c.closeCh
}

// Err implements context.Context
func (c *candidateBase) Err() error {
    select {
    case <-c.closedCh:
        return ErrRunCanceled
    default:
        return nil
    }
}

// Deadline implements context.Context
func (c *candidateBase) Deadline() (deadline time.Time, ok bool) {
    return time.Time{}, false
}

// Value implements context.Context
func (c *candidateBase) Value(key interface{}) interface{} {
    return nil
}
*/

impl Default for CandidateBase {
    fn default() -> Self {
        CandidateBase {
            id: String::new(),
            network_type: Arc::new(AtomicU8::new(0)),
            candidate_type: CandidateType::default(),

            component: Arc::new(AtomicU16::new(0)),
            address: String::new(),
            port: 0,
            related_address: None,
            tcp_type: TCPType::default(),

            resolved_addr: SocketAddr::new(IpAddr::from([0, 0, 0, 0]), 0),

            last_sent: Arc::new(AtomicU64::new(0)),
            last_received: Arc::new(AtomicU64::new(0)),
            conn: None,
            //TODO:currAgent :Option<Agent>,
            //TODO:closeCh   chan struct{}
            //TODO:closedCh  chan struct{}
            foundation_override: String::new(),
            priority_override: 0,
            network: String::new(),
            relay_client: None,
        }
    }
}

// String makes the candidateBase printable
impl fmt::Display for CandidateBase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(related_address) = self.related_address() {
            write!(
                f,
                "{} {} {}:{}{}",
                self.network_type(),
                self.candidate_type(),
                self.address(),
                self.port(),
                related_address,
            )
        } else {
            write!(
                f,
                "{} {} {}:{}",
                self.network_type(),
                self.candidate_type(),
                self.address(),
                self.port(),
            )
        }
    }
}

#[async_trait]
impl Candidate for CandidateBase {
    fn foundation(&self) -> String {
        if !self.foundation_override.is_empty() {
            return self.foundation_override.clone();
        }

        let mut buf = vec![];
        buf.extend_from_slice(self.candidate_type().to_string().as_bytes());
        buf.extend_from_slice(self.network_type().to_string().as_bytes());

        let mut hasher = Hasher::new();
        hasher.update(&buf);
        let checksum = hasher.finalize();

        format!("{}", checksum)
    }

    // ID returns Candidate ID
    fn id(&self) -> String {
        self.id.clone()
    }

    // Component returns candidate component
    fn component(&self) -> u16 {
        self.component.load(Ordering::SeqCst)
    }

    fn set_component(&self, component: u16) {
        self.component.store(component, Ordering::SeqCst);
    }

    // LastReceived returns a time.Time indicating the last time
    // this candidate was received
    fn last_received(&self) -> SystemTime {
        UNIX_EPOCH.add(Duration::from_nanos(
            self.last_received.load(Ordering::SeqCst),
        ))
    }

    // LastSent returns a time.Time indicating the last time
    // this candidate was sent
    fn last_sent(&self) -> SystemTime {
        UNIX_EPOCH.add(Duration::from_nanos(self.last_sent.load(Ordering::SeqCst)))
    }

    // NetworkType returns candidate NetworkType
    fn network_type(&self) -> NetworkType {
        NetworkType::from(self.network_type.load(Ordering::SeqCst))
    }

    // Address returns Candidate Address
    fn address(&self) -> String {
        self.address.clone()
    }

    // Port returns Candidate Port
    fn port(&self) -> u16 {
        self.port
    }

    // Priority computes the priority for this ICE Candidate
    fn priority(&self) -> u32 {
        if self.priority_override != 0 {
            return self.priority_override;
        }

        // The local preference MUST be an integer from 0 (lowest preference) to
        // 65535 (highest preference) inclusive.  When there is only a single IP
        // address, this value SHOULD be set to 65535.  If there are multiple
        // candidates for a particular component for a particular data stream
        // that have the same type, the local preference MUST be unique for each
        // one.
        (1 << 24) * (self.candidate_type().preference() as u32)
            + (1 << 8) * (self.local_preference() as u32)
            + (256 - self.component() as u32)
    }

    // RelatedAddress returns *CandidateRelatedAddress
    fn related_address(&self) -> Option<CandidateRelatedAddress> {
        if let Some(related_address) = &self.related_address {
            Some(related_address.clone())
        } else {
            None
        }
    }

    // Type returns candidate type
    fn candidate_type(&self) -> CandidateType {
        self.candidate_type
    }

    fn tcp_type(&self) -> TCPType {
        self.tcp_type
    }

    // Marshal returns the string representation of the ICECandidate
    fn marshal(&self) -> String {
        let mut val = format!(
            "{} {} {} {} {} {} typ {}",
            self.foundation(),
            self.component(),
            self.network_type().network_short(),
            self.priority(),
            self.address(),
            self.port(),
            self.candidate_type()
        );

        if self.tcp_type != TCPType::Unspecified {
            val += format!(" tcptype {}", self.tcp_type()).as_str();
        }

        if let Some(related_address) = self.related_address() {
            val += format!(
                " raddr {} rport {}",
                related_address.address, related_address.port,
            )
            .as_str();
        }

        val
    }

    fn addr(&self) -> SocketAddr {
        self.resolved_addr
    }

    /*TODO: func (c *candidateBase) agent() *Agent {
        return c.currAgent
    }

    func (c *candidateBase) context() context.Context {
        return c
    }*/

    // close stops the recvLoop
    async fn close(&self) -> Result<(), Error> {
        //TODO:
        // If conn has never been started will be nil
        /*if c.Done() == nil {
            return nil
        }

        // Assert that conn has not already been closed
        select {
        case <-c.Done():
            return nil
        default:
        }

        var firstErr error

        // Unblock recvLoop
        close(c.closeCh)
        if err := c.conn.SetDeadline(time.Now()); err != nil {
            firstErr = err
        }

        // Close the conn
        if err := c.conn.Close(); err != nil && firstErr == nil {
            firstErr = err
        }

        if firstErr != nil {
            return firstErr
        }

        // Wait until the recvLoop is closed
        <-c.closedCh*/

        if let Some(relay_client) = &self.relay_client {
            relay_client.close().await
        } else {
            Ok(())
        }
    }

    fn seen(&self, outbound: bool) {
        let d = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(d) => d,
            Err(_) => Duration::from_secs(0),
        };

        if outbound {
            self.set_last_sent(d)
        } else {
            self.set_last_received(d)
        }
    }

    async fn write_to(
        &self,
        raw: &[u8],
        dst: &(dyn Candidate + Send + Sync),
    ) -> Result<usize, Error> {
        let n = if let Some(conn) = &self.conn {
            let addr = dst.addr();
            conn.send_to(raw, addr).await?
        } else {
            0
        };
        self.seen(true);
        Ok(n)
    }

    // Equal is used to compare two candidateBases
    fn equal(&self, other: &dyn Candidate) -> bool {
        self.network_type() == other.network_type()
            && self.candidate_type() == other.candidate_type()
            && self.address() == other.address()
            && self.port() == other.port()
            && self.tcp_type() == other.tcp_type()
            && self.related_address() == other.related_address()
    }

    fn clone(&self) -> Arc<dyn Candidate + Send + Sync> {
        Arc::new(Clone::clone(self))
    }

    fn clone_with_ip(&self, ip: &IpAddr) -> Arc<dyn Candidate + Send + Sync> {
        let network_type = match determine_network_type(&self.network, ip) {
            Ok(network_type) => network_type,
            Err(_) => {
                if ip.is_ipv4() {
                    NetworkType::UDP4
                } else {
                    NetworkType::UDP6
                }
            }
        };

        let resolved_addr = create_addr(network_type, *ip, self.port);

        Arc::new(CandidateBase {
            id: self.id.clone(),
            network_type: Arc::new(AtomicU8::new(network_type as u8)),
            candidate_type: self.candidate_type,

            component: Arc::clone(&self.component),
            address: self.address.clone(),
            port: self.port,
            related_address: self.related_address.clone(),
            tcp_type: self.tcp_type,

            resolved_addr,

            last_sent: Arc::clone(&self.last_sent),
            last_received: Arc::clone(&self.last_received),
            conn: self.conn.clone(),
            //TODO:currAgent :Option<Agent>,
            //TODO:closeCh   chan struct{}
            //TODO:closedCh  chan struct{}
            foundation_override: self.foundation_override.clone(),
            priority_override: self.priority_override,
            network: self.network.clone(),
            relay_client: self.relay_client.clone(),
        })
    }

    // start runs the candidate using the provided connection
    async fn start(&self, initialized_ch: Option<broadcast::Receiver<()>>) {
        if let Some(conn) = &self.conn {
            let conn = Arc::clone(conn);
            tokio::spawn(async move {
                let _ = CandidateBase::recv_loop(initialized_ch, conn).await;
            });
        } else {
            log::error!("Can't start due to conn is_none");
        }
    }
}

impl CandidateBase {
    pub fn set_ip(&mut self, ip: &IpAddr) -> Result<(), Error> {
        let network_type = determine_network_type(&self.network, ip)?;

        self.network_type
            .store(network_type as u8, Ordering::SeqCst);
        self.resolved_addr = create_addr(network_type, *ip, self.port);

        Ok(())
    }

    pub fn set_last_received(&self, d: Duration) {
        self.last_received
            .store(d.as_nanos() as u64, Ordering::SeqCst);
    }

    pub fn set_last_sent(&self, d: Duration) {
        self.last_sent.store(d.as_nanos() as u64, Ordering::SeqCst);
    }

    // LocalPreference returns the local preference for this candidate
    pub fn local_preference(&self) -> u16 {
        if self.network_type().is_tcp() {
            // RFC 6544, section 4.2
            //
            // In Section 4.1.2.1 of [RFC5245], a recommended formula for UDP ICE
            // candidate prioritization is defined.  For TCP candidates, the same
            // formula and candidate type preferences SHOULD be used, and the
            // RECOMMENDED type preferences for the new candidate types defined in
            // this document (see Section 5) are 105 for NAT-assisted candidates and
            // 75 for UDP-tunneled candidates.
            //
            // (...)
            //
            // With TCP candidates, the local preference part of the recommended
            // priority formula is updated to also include the directionality
            // (active, passive, or simultaneous-open) of the TCP connection.  The
            // RECOMMENDED local preference is then defined as:
            //
            //     local preference = (2^13) * direction-pref + other-pref
            //
            // The direction-pref MUST be between 0 and 7 (both inclusive), with 7
            // being the most preferred.  The other-pref MUST be between 0 and 8191
            // (both inclusive), with 8191 being the most preferred.  It is
            // RECOMMENDED that the host, UDP-tunneled, and relayed TCP candidates
            // have the direction-pref assigned as follows: 6 for active, 4 for
            // passive, and 2 for S-O.  For the NAT-assisted and server reflexive
            // candidates, the RECOMMENDED values are: 6 for S-O, 4 for active, and
            // 2 for passive.
            //
            // (...)
            //
            // If any two candidates have the same type-preference and direction-
            // pref, they MUST have a unique other-pref.  With this specification,
            // this usually only happens with multi-homed hosts, in which case
            // other-pref is the preference for the particular IP address from which
            // the candidate was obtained.  When there is only a single IP address,
            // this value SHOULD be set to the maximum allowed value (8191).
            let other_pref: u16 = 8191;

            let direction_pref: u16 = match self.candidate_type() {
                CandidateType::Host | CandidateType::Relay => match self.tcp_type() {
                    TCPType::Active => 6,
                    TCPType::Passive => 4,
                    TCPType::SimultaneousOpen => 2,
                    TCPType::Unspecified => 0,
                },
                CandidateType::PeerReflexive | CandidateType::ServerReflexive => {
                    match self.tcp_type() {
                        TCPType::SimultaneousOpen => 6,
                        TCPType::Active => 4,
                        TCPType::Passive => 2,
                        TCPType::Unspecified => 0,
                    }
                }
                CandidateType::Unspecified => 0,
            };

            (1 << 13) * direction_pref + other_pref
        } else {
            DEFAULT_LOCAL_PREFERENCE
        }
    }

    async fn recv_loop(
        initialized_ch: Option<broadcast::Receiver<()>>,
        conn: Arc<dyn util::Conn + Send + Sync>,
    ) -> Result<(), Error> {
        /*defer func() {
            close(c.closedCh)
        }()*/

        /*select {
        case <-initialized_ch:
        case <-c.closeCh:
            return
        }*/
        if let Some(mut initialized_ch) = initialized_ch {
            let _ = initialized_ch.recv().await;
        }

        let mut buffer = vec![0u8; RECEIVE_MTU];
        loop {
            let (n, src_addr) = conn.recv_from(&mut buffer).await?;

            CandidateBase::handle_inbound_candidate_msg(&buffer[..n], src_addr);
        }
    }

    fn handle_inbound_candidate_msg(_buffer: &[u8], _src_addr: SocketAddr) {
        /*TODO: if stun.IsMessage(buffer) {
            m := &stun.Message{
                Raw: make([]byte, len(buffer)),
            }
            // Explicitly copy raw buffer so Message can own the memory.
            copy(m.Raw, buffer)
            if err := m.Decode(); err != nil {
                log.Warnf("Failed to handle decode ICE from %s to %s: %v", c.addr(), srcAddr, err)
                return
            }
            err := c.agent().run(ctx, func(ctx context.Context, agent *Agent) {
                agent.handleInbound(m, c, srcAddr)
            })
            if err != nil {
                log.Warnf("Failed to handle message: %v", err)
            }

            return
        }

        if !c.agent().validate_non_stuntraffic(c, srcAddr) {
            log.Warnf("Discarded message from %s, not a valid remote candidate", c.addr())
            return
        }

        // NOTE This will return packetio.ErrFull if the buffer ever manages to fill up.
        if _, err := c.agent().buffer.Write(buffer); err != nil {
            log.Warnf("failed to write packet")
        }*/
    }
}

/*
// UnmarshalCandidate creates a Candidate from its string representation
func UnmarshalCandidate(raw string) (Candidate, error) {
    split := strings.Fields(raw)
    if len(split) < 8 {
        return nil, fmt.Errorf("%w (%d)", errAttributeTooShortICECandidate, len(split))
    }

    // Foundation
    foundation := split[0]

    // Component
    rawComponent, err := strconv.ParseUint(split[1], 10, 16)
    if err != nil {
        return nil, fmt.Errorf("%w: %v", errParseComponent, err)
    }
    component := uint16(rawComponent)

    // Protocol
    protocol := split[2]

    // Priority
    priorityRaw, err := strconv.ParseUint(split[3], 10, 32)
    if err != nil {
        return nil, fmt.Errorf("%w: %v", errParsePriority, err)
    }
    priority := uint32(priorityRaw)

    // Address
    address := split[4]

    // Port
    rawPort, err := strconv.ParseUint(split[5], 10, 16)
    if err != nil {
        return nil, fmt.Errorf("%w: %v", errParsePort, err)
    }
    port := int(rawPort)
    typ := split[7]

    relatedAddress := ""
    relatedPort := 0
    tcpType := TCPTypeUnspecified

    if len(split) > 8 {
        split = split[8:]

        if split[0] == "raddr" {
            if len(split) < 4 {
                return nil, fmt.Errorf("%w: incorrect length", errParseRelatedAddr)
            }

            // RelatedAddress
            relatedAddress = split[1]

            // RelatedPort
            rawRelatedPort, parseErr := strconv.ParseUint(split[3], 10, 16)
            if parseErr != nil {
                return nil, fmt.Errorf("%w: %v", errParsePort, parseErr)
            }
            relatedPort = int(rawRelatedPort)
        } else if split[0] == "tcptype" {
            if len(split) < 2 {
                return nil, fmt.Errorf("%w: incorrect length", errParseTypType)
            }

            tcpType = NewTCPType(split[1])
        }
    }

    switch typ {
    case "host":
        return NewCandidateHost(&CandidateHostConfig{"", protocol, address, port, component, priority, foundation, tcpType})
    case "srflx":
        return new_candidate_server_reflexive(&CandidateServerReflexiveConfig{"", protocol, address, port, component, priority, foundation, relatedAddress, relatedPort})
    case "prflx":
        return new_candidate_peer_reflexive(&CandidatePeerReflexiveConfig{"", protocol, address, port, component, priority, foundation, relatedAddress, relatedPort})
    case "relay":
        return new_candidate_relay(&CandidateRelayConfig{"", protocol, address, port, component, priority, foundation, relatedAddress, relatedPort, nil})
    default:
    }

    return nil, fmt.Errorf("%w (%s)", errUnknownCandidateTyp, typ)
}
*/
