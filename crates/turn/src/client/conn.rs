// client implements the API for a TURN client
use super::binding::*;
use super::periodic_timer::*;
use super::permission::*;
use super::transaction::*;
use crate::proto;

use crate::errors::*;

use stun::agent::*;
use stun::attributes::*;
use stun::error_code::*;
use stun::fingerprint::*;
use stun::integrity::*;
use stun::message::*;
use stun::textattrs::*;

use util::Error;

use std::net::SocketAddr;

use tokio::sync::mpsc;
use tokio::time::{Duration, Instant};

const MAX_READ_QUEUE_SIZE: usize = 1024;
const PERM_REFRESH_INTERVAL: Duration = Duration::from_secs(120);
const MAX_RETRY_ATTEMPTS: u16 = 3;

enum TimerIdRefresh {
    Alloc = 0,
    Perms = 1,
}

struct InboundData {
    data: Vec<u8>,
    from: SocketAddr,
}

// UDPConnObserver is an interface to UDPConn observer
pub trait UDPConnObserver {
    fn turn_server_addr(&self) -> SocketAddr;
    fn username(&self) -> Username;
    fn realm(&self) -> Realm;
    fn write_to(&self, data: &[u8], to: SocketAddr) -> Result<usize, Error>;
    fn perform_transaction(
        &self,
        msg: &Message,
        to: SocketAddr,
        dont_wait: bool,
    ) -> Result<TransactionResult, Error>;
    fn on_deallocated(&self, relayed_addr: SocketAddr);
}

// UDPConnConfig is a set of configuration params use by NewUDPConn
pub struct UDPConnConfig {
    observer: Box<dyn UDPConnObserver>,
    relayed_addr: SocketAddr,
    integrity: MessageIntegrity,
    nonce: Nonce,
    lifetime: Duration,
}

// UDPConn is the implementation of the Conn and PacketConn interfaces for UDP network connections.
// compatible with net.PacketConn and net.Conn
pub struct UDPConn {
    obs: Box<dyn UDPConnObserver>,
    relayed_addr: SocketAddr,
    perm_map: PermissionMap,
    binding_mgr: BindingManager,
    integrity: MessageIntegrity,
    nonce: Nonce,
    lifetime: Duration,
    read_ch_tx: mpsc::Sender<InboundData>,
    read_ch_rx: mpsc::Receiver<InboundData>,
    close_ch_tx: Option<mpsc::Sender<()>>,
    close_ch_rx: mpsc::Receiver<()>,
    refresh_alloc_timer: PeriodicTimer,
    refresh_perms_timer: PeriodicTimer,
}

impl UDPConn {
    // new creates a new instance of UDPConn
    pub fn new(config: UDPConnConfig) -> Self {
        let (read_ch_tx, read_ch_rx) = mpsc::channel(MAX_READ_QUEUE_SIZE);
        let (close_ch_tx, close_ch_rx) = mpsc::channel(1);

        let mut c = UDPConn {
            obs: config.observer,
            relayed_addr: config.relayed_addr,
            perm_map: PermissionMap::new(),
            binding_mgr: BindingManager::new(),
            integrity: config.integrity,
            nonce: config.nonce,
            lifetime: config.lifetime,
            read_ch_tx,
            read_ch_rx,
            close_ch_tx: Some(close_ch_tx),
            close_ch_rx,
            refresh_alloc_timer: PeriodicTimer::new(
                TimerIdRefresh::Alloc as usize,
                None, //TODO
                config.lifetime / 2,
            ),
            refresh_perms_timer: PeriodicTimer::new(
                TimerIdRefresh::Perms as usize,
                None, //TODO
                PERM_REFRESH_INTERVAL,
            ),
        };

        log::debug!("initial lifetime: {} seconds", c.lifetime.as_secs());

        if c.refresh_alloc_timer.start() {
            log::debug!("refresh_alloc_timer started");
        }
        if c.refresh_perms_timer.start() {
            log::debug!("refresh_perms_timer started");
        }

        c
    }

    // ReadFrom reads a packet from the connection,
    // copying the payload into p. It returns the number of
    // bytes copied into p and the return address that
    // was on the packet.
    // It returns the number of bytes read (0 <= n <= len(p))
    // and any error encountered. Callers should always process
    // the n > 0 bytes returned before considering the error err.
    // ReadFrom can be made to time out and return
    // an Error with Timeout() == true after a fixed time limit;
    // see SetDeadline and SetReadDeadline.
    pub async fn recv_from(&mut self, p: &mut [u8]) -> Result<(usize, SocketAddr), Error> {
        loop {
            tokio::select! {
                result = self.read_ch_rx.recv() => if let Some(ib_data) = result{
                    let n = ib_data.data.len();
                    if p.len() <  n {
                        return Err(ERR_SHORT_BUFFER.to_owned());
                    }
                    p[..n].copy_from_slice(&ib_data.data);
                    return Ok((n, ib_data.from));
                },
                _ = self.close_ch_rx.recv() => return Err(ERR_CLOSED.to_owned()),
            }
        }
    }

    // write_to writes a packet with payload p to addr.
    // write_to can be made to time out and return
    // an Error with Timeout() == true after a fixed time limit;
    // see SetDeadline and SetWriteDeadline.
    // On packet-oriented connections, write timeouts are rare.
    pub fn send_to(&mut self, p: &[u8], addr: SocketAddr) -> Result<usize, Error> {
        // check if we have a permission for the destination IP addr
        let mut perm = if let Some(perm) = self.perm_map.find(&addr) {
            *perm
        } else {
            let perm = Permission::default();
            self.perm_map.insert(&addr, perm);
            perm
        };

        // This func-block would block, per destination IP (, or perm), until
        // the perm state becomes "requested". Purpose of this is to guarantee
        // the order of packets (within the same perm).
        // Note that CreatePermission transaction may not be complete before
        // all the data transmission. This is done assuming that the request
        // will be mostly likely successful and we can tolerate some loss of
        // UDP packet (or reorder), inorder to minimize the latency in most cases.
        let mut create_permission = || -> Result<(), Error> {
            if perm.state() == PermState::Idle {
                // punch a hole! (this would block a bit..)
                if let Err(err) = self.create_permissions(&[addr]) {
                    self.perm_map.delete(&addr);
                    return Err(err);
                }
                perm.set_state(PermState::Permitted);
            }
            Ok(())
        };

        for _ in 0..MAX_RETRY_ATTEMPTS {
            if let Err(err) = create_permission() {
                if err == *ERR_TRY_AGAIN {
                    break;
                } else {
                    return Err(err);
                }
            }
        }

        // bind channel
        if self.binding_mgr.find_by_addr(&addr).is_none() {
            self.binding_mgr.create(addr);
        }

        let number = {
            let b = self
                .binding_mgr
                .get_by_addr(&addr)
                .ok_or_else(|| Error::new("Addr not found".to_owned()))?;

            let bind_st = b.state();

            if bind_st == BindingState::Idle
                || bind_st == BindingState::Request
                || bind_st == BindingState::Failed
            {
                let mut f = || {
                    // block only callers with the same binding until
                    // the binding transaction has been complete
                    // binding state may have been changed while waiting. check again.
                    if b.state() == BindingState::Idle {
                        b.set_state(BindingState::Request);
                        /*TODO: go func() {
                            err2 := c.bind(b)
                            if err2 != nil {
                                c.log.Warnf("bind() failed: %s", err2.Error())
                                b.setState(bindingStateFailed)
                                // keep going...
                            } else {
                                b.setState(bindingStateReady)
                            }
                        }()*/
                    }
                };
                f();

                // send data using SendIndication
                let peer_addr = socket_addr2peer_address(&addr);
                let mut msg = Message::new();
                msg.build(&[
                    Box::new(TransactionId::new()),
                    Box::new(MessageType::new(METHOD_SEND, CLASS_INDICATION)),
                    Box::new(proto::data::Data(p.to_vec())),
                    Box::new(peer_addr),
                    Box::new(FINGERPRINT),
                ])?;

                // indication has no transaction (fire-and-forget)

                return self.obs.write_to(&msg.raw, self.obs.turn_server_addr());
            }

            // binding is either ready

            // check if the binding needs a refresh
            let mut f = || {
                if b.state() == BindingState::Ready
                    && Instant::now().duration_since(b.refreshed_at()) > Duration::from_secs(5 * 60)
                {
                    b.set_state(BindingState::Refresh);
                    /*TODO: go func() {
                        err = c.bind(b)
                        if err != nil {
                            c.log.Warnf("bind() for refresh failed: %s", err.Error())
                            b.setState(bindingStateFailed)
                            // keep going...
                        } else {
                            b.setRefreshedAt(time.Now())
                            b.setState(bindingStateReady)
                        }
                    }()*/
                }
            };
            f();

            b.number
        };

        // send via ChannelData
        self.send_channel_data(p, number)
    }

    fn send_channel_data(&self, data: &[u8], ch_num: u16) -> Result<usize, Error> {
        let mut ch_data = proto::chandata::ChannelData {
            data: data.to_vec(),
            number: proto::channum::ChannelNumber(ch_num),
            ..Default::default()
        };
        ch_data.encode();
        self.obs.write_to(&ch_data.raw, self.obs.turn_server_addr())
    }

    fn create_permissions(&mut self, addrs: &[SocketAddr]) -> Result<(), Error> {
        let mut setters: Vec<Box<dyn Setter>> = vec![
            Box::new(TransactionId::new()),
            Box::new(MessageType::new(METHOD_CREATE_PERMISSION, CLASS_REQUEST)),
        ];

        for addr in addrs {
            setters.push(Box::new(socket_addr2peer_address(addr)));
        }

        setters.push(Box::new(self.obs.username()));
        setters.push(Box::new(self.obs.realm()));
        setters.push(Box::new(self.nonce.clone()));
        setters.push(Box::new(self.integrity.clone()));
        setters.push(Box::new(FINGERPRINT));

        let mut msg = Message::new();
        msg.build(&setters)?;

        let tr_res = self
            .obs
            .perform_transaction(&msg, self.obs.turn_server_addr(), false)?;

        let res = tr_res.msg;

        if res.typ.class == CLASS_ERROR_RESPONSE {
            let mut code = ErrorCodeAttribute::default();
            let result = code.get_from(&res);
            if result.is_err() {
                return Err(Error::new(format!("{}", res.typ)));
            } else if code.code == CODE_STALE_NONCE {
                self.set_nonce_from_msg(&res);
                return Err(ERR_TRY_AGAIN.to_owned());
            } else {
                return Err(Error::new(format!("{} (error {})", res.typ, code)));
            }
        }

        Ok(())
    }

    pub fn set_nonce_from_msg(&mut self, msg: &Message) {
        // Update nonce
        match Nonce::get_from_as(msg, ATTR_NONCE) {
            Ok(nonce) => {
                self.nonce = nonce;
                log::debug!("refresh allocation: 438, got new nonce.");
            }
            Err(_) => log::warn!("refresh allocation: 438 but no nonce."),
        }
    }

    // LocalAddr returns the local network address.
    pub fn local_addr(&self) -> SocketAddr {
        self.relayed_addr
    }

    // Close closes the connection.
    // Any blocked ReadFrom or write_to operations will be unblocked and return errors.
    pub fn close(&mut self) -> Result<(), Error> {
        if self.close_ch_tx.is_none() {
            return Err(ERR_ALREADY_CLOSED.to_owned());
        }

        self.refresh_alloc_timer.stop();
        self.refresh_perms_timer.stop();
        self.close_ch_tx.take();

        self.obs.on_deallocated(self.relayed_addr);
        self.refresh_allocation(Duration::from_secs(0), true /* dontWait=true */)
    }

    // handle_inbound passes inbound data in UDPConn
    pub fn handle_inbound(&mut self, data: &[u8], from: SocketAddr) {
        if self
            .read_ch_tx
            .try_send(InboundData {
                data: data.to_vec(),
                from,
            })
            .is_err()
        {
            log::warn!("receive buffer full");
        }
    }

    // find_addr_by_channel_number returns a peer address associated with the
    // channel number on this UDPConn
    pub fn find_addr_by_channel_number(&self, ch_num: u16) -> Option<SocketAddr> {
        if let Some(b) = self.binding_mgr.find_by_number(ch_num) {
            Some(b.addr)
        } else {
            None
        }
    }

    fn refresh_allocation(&mut self, lifetime: Duration, dont_wait: bool) -> Result<(), Error> {
        let mut msg = Message::new();
        msg.build(&[
            Box::new(TransactionId::new()),
            Box::new(MessageType::new(METHOD_REFRESH, CLASS_REQUEST)),
            Box::new(proto::lifetime::Lifetime(lifetime)),
            Box::new(self.obs.username()),
            Box::new(self.obs.realm()),
            Box::new(self.nonce.clone()),
            Box::new(self.integrity.clone()),
            Box::new(FINGERPRINT),
        ])?;

        log::debug!("send refresh request (dont_wait={})", dont_wait);
        let tr_res = self
            .obs
            .perform_transaction(&msg, self.obs.turn_server_addr(), dont_wait)?;

        if dont_wait {
            log::debug!("refresh request sent");
            return Ok(());
        }

        log::debug!("refresh request sent, and waiting response");

        let res = tr_res.msg;
        if res.typ.class == CLASS_ERROR_RESPONSE {
            let mut code = ErrorCodeAttribute::default();
            let result = code.get_from(&res);
            if result.is_err() {
                return Err(Error::new(format!("{}", res.typ)));
            } else if code.code == CODE_STALE_NONCE {
                self.set_nonce_from_msg(&res);
                return Err(ERR_TRY_AGAIN.to_owned());
            } else {
                return Ok(());
            }
        }

        // Getting lifetime from response
        let mut updated_lifetime = proto::lifetime::Lifetime::default();
        updated_lifetime.get_from(&res)?;

        self.lifetime = updated_lifetime.0;
        log::debug!("updated lifetime: {} seconds", self.lifetime.as_secs());
        Ok(())
    }

    fn refresh_permissions(&mut self) -> Result<(), Error> {
        let addrs = self.perm_map.addrs();
        if addrs.is_empty() {
            log::debug!("no permission to refresh");
            return Ok(());
        }

        if let Err(err) = self.create_permissions(&addrs) {
            if err != *ERR_TRY_AGAIN {
                log::error!("fail to refresh permissions: {}", err);
            }
            return Err(err);
        }

        log::debug!("refresh permissions successful");
        Ok(())
    }

    fn bind(&mut self, b: &Binding) -> Result<(), Error> {
        let setters: Vec<Box<dyn Setter>> = vec![
            Box::new(TransactionId::new()),
            Box::new(MessageType::new(METHOD_CHANNEL_BIND, CLASS_REQUEST)),
            Box::new(socket_addr2peer_address(&b.addr)),
            Box::new(proto::channum::ChannelNumber(b.number)),
            Box::new(self.obs.username()),
            Box::new(self.obs.realm()),
            Box::new(self.nonce.clone()),
            Box::new(self.integrity.clone()),
            Box::new(FINGERPRINT),
        ];

        let mut msg = Message::new();
        msg.build(&setters)?;

        let tr_res = match self
            .obs
            .perform_transaction(&msg, self.obs.turn_server_addr(), false)
        {
            Err(err) => {
                self.binding_mgr.delete_by_addr(&b.addr);
                return Err(err);
            }
            Ok(tr_res) => tr_res,
        };

        let res = tr_res.msg;

        if res.typ != MessageType::new(METHOD_CHANNEL_BIND, CLASS_SUCCESS_RESPONSE) {
            return Err(Error::new(format!("unexpected response type {}", res.typ)));
        }

        log::debug!("channel binding successful: {} {}", b.addr, b.number);

        // Success.
        Ok(())
    }
}

/*

func (c *UDPConn) onRefreshTimers(id int) {
    c.log.Debugf("refresh timer %d expired", id)
    switch id {
    case timerIDRefreshAlloc:
        var err error
        lifetime := c.lifetime()
        // limit the max retries on errTryAgain to 3
        // when stale nonce returns, sencond retry should succeed
        for i := 0; i < MAX_RETRY_ATTEMPTS; i++ {
            err = c.refresh_allocation(lifetime, false)
            if !errors.Is(err, errTryAgain) {
                break
            }
        }
        if err != nil {
            c.log.Warnf("refresh allocation failed")
        }
    case timerIDRefreshPerms:
        var err error
        for i := 0; i < MAX_RETRY_ATTEMPTS; i++ {
            err = c.refresh_permissions()
            if !errors.Is(err, errTryAgain) {
                break
            }
        }
        if err != nil {
            c.log.Warnf("refresh permissions failed")
        }
    }
}

*/

fn socket_addr2peer_address(addr: &SocketAddr) -> proto::peeraddr::PeerAddress {
    proto::peeraddr::PeerAddress {
        ip: addr.ip(),
        port: addr.port(),
    }
}
