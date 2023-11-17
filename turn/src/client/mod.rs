#[cfg(test)]
mod client_test;

pub mod binding;
pub mod periodic_timer;
pub mod permission;
pub mod relay_conn;
pub mod transaction;

use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;

use async_trait::async_trait;
use base64::prelude::BASE64_STANDARD;
use base64::Engine;
use binding::*;
use relay_conn::*;
use stun::agent::*;
use stun::attributes::*;
use stun::error_code::*;
use stun::fingerprint::*;
use stun::integrity::*;
use stun::message::*;
use stun::textattrs::*;
use stun::xoraddr::*;
use tokio::pin;
use tokio::select;
use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;
use transaction::*;
use util::conn::*;
use util::vnet::net::*;

use crate::error::*;
use crate::proto::chandata::*;
use crate::proto::data::*;
use crate::proto::lifetime::*;
use crate::proto::peeraddr::*;
use crate::proto::relayaddr::*;
use crate::proto::reqtrans::*;
use crate::proto::PROTO_UDP;

const DEFAULT_RTO_IN_MS: u16 = 200;
const MAX_DATA_BUFFER_SIZE: usize = u16::MAX as usize; // message size limit for Chromium
const MAX_READ_QUEUE_SIZE: usize = 1024;

//              interval [msec]
// 0: 0 ms      +500
// 1: 500 ms	+1000
// 2: 1500 ms   +2000
// 3: 3500 ms   +4000
// 4: 7500 ms   +8000
// 5: 15500 ms  +16000
// 6: 31500 ms  +32000
// -: 63500 ms  failed

/// ClientConfig is a bag of config parameters for Client.
pub struct ClientConfig {
    pub stun_serv_addr: String, // STUN server address (e.g. "stun.abc.com:3478")
    pub turn_serv_addr: String, // TURN server address (e.g. "turn.abc.com:3478")
    pub username: String,
    pub password: String,
    pub realm: String,
    pub software: String,
    pub rto_in_ms: u16,
    pub conn: Arc<dyn Conn + Send + Sync>,
    pub vnet: Option<Arc<Net>>,
}

struct ClientInternal {
    conn: Arc<dyn Conn + Send + Sync>,
    stun_serv_addr: String,
    turn_serv_addr: String,
    username: Username,
    password: String,
    realm: Realm,
    integrity: MessageIntegrity,
    software: Software,
    tr_map: Arc<Mutex<TransactionMap>>,
    binding_mgr: Arc<Mutex<BindingManager>>,
    rto_in_ms: u16,
    read_ch_tx: Arc<Mutex<Option<mpsc::Sender<InboundData>>>>,
    close_notify: CancellationToken,
}

#[async_trait]
impl RelayConnObserver for ClientInternal {
    /// Returns the TURN server address.
    fn turn_server_addr(&self) -> String {
        self.turn_serv_addr.clone()
    }

    /// Returns the `username`.
    fn username(&self) -> Username {
        self.username.clone()
    }

    /// Return the `realm`.
    fn realm(&self) -> Realm {
        self.realm.clone()
    }

    /// Sends data to the specified destination using the base socket.
    async fn write_to(&self, data: &[u8], to: &str) -> std::result::Result<usize, util::Error> {
        let n = self.conn.send_to(data, SocketAddr::from_str(to)?).await?;
        Ok(n)
    }

    /// Performs STUN transaction.
    async fn perform_transaction(
        &mut self,
        msg: &Message,
        to: &str,
        ignore_result: bool,
    ) -> Result<TransactionResult> {
        let tr_key = BASE64_STANDARD.encode(msg.transaction_id.0);

        let mut tr = Transaction::new(TransactionConfig {
            key: tr_key.clone(),
            raw: msg.raw.clone(),
            to: to.to_string(),
            interval: self.rto_in_ms,
            ignore_result,
        });
        let result_ch_rx = tr.get_result_channel();

        log::trace!("start {} transaction {} to {}", msg.typ, tr_key, tr.to);
        {
            let mut tm = self.tr_map.lock().await;
            tm.insert(tr_key.clone(), tr);
        }

        self.conn
            .send_to(&msg.raw, SocketAddr::from_str(to)?)
            .await?;

        let conn2 = Arc::clone(&self.conn);
        let tr_map2 = Arc::clone(&self.tr_map);
        {
            let mut tm = self.tr_map.lock().await;
            if let Some(tr) = tm.get(&tr_key) {
                tr.start_rtx_timer(conn2, tr_map2).await;
            }
        }

        // If dontWait is true, get the transaction going and return immediately
        if ignore_result {
            return Ok(TransactionResult::default());
        }

        // wait_for_result waits for the transaction result
        if let Some(mut result_ch_rx) = result_ch_rx {
            match result_ch_rx.recv().await {
                Some(tr) => Ok(tr),
                None => Err(Error::ErrTransactionClosed),
            }
        } else {
            Err(Error::ErrWaitForResultOnNonResultTransaction)
        }
    }
}

impl ClientInternal {
    /// Creates a new [`ClientInternal`].
    async fn new(config: ClientConfig) -> Result<Self> {
        let net = if let Some(vnet) = config.vnet {
            if vnet.is_virtual() {
                log::warn!("vnet is enabled");
            }
            vnet
        } else {
            Arc::new(Net::new(None))
        };

        let stun_serv_addr = if config.stun_serv_addr.is_empty() {
            String::new()
        } else {
            log::debug!("resolving {}", config.stun_serv_addr);
            let local_addr = config.conn.local_addr()?;
            let stun_serv = net
                .resolve_addr(local_addr.is_ipv4(), &config.stun_serv_addr)
                .await?;
            log::debug!("stunServ: {}", stun_serv);
            stun_serv.to_string()
        };

        let turn_serv_addr = if config.turn_serv_addr.is_empty() {
            String::new()
        } else {
            log::debug!("resolving {}", config.turn_serv_addr);
            let local_addr = config.conn.local_addr()?;
            let turn_serv = net
                .resolve_addr(local_addr.is_ipv4(), &config.turn_serv_addr)
                .await?;
            log::debug!("turnServ: {}", turn_serv);
            turn_serv.to_string()
        };

        Ok(ClientInternal {
            conn: Arc::clone(&config.conn),
            stun_serv_addr,
            turn_serv_addr,
            username: Username::new(ATTR_USERNAME, config.username),
            password: config.password,
            realm: Realm::new(ATTR_REALM, config.realm),
            software: Software::new(ATTR_SOFTWARE, config.software),
            tr_map: Arc::new(Mutex::new(TransactionMap::new())),
            binding_mgr: Arc::new(Mutex::new(BindingManager::new())),
            rto_in_ms: if config.rto_in_ms != 0 {
                config.rto_in_ms
            } else {
                DEFAULT_RTO_IN_MS
            },
            integrity: MessageIntegrity::new_short_term_integrity(String::new()),
            read_ch_tx: Arc::new(Mutex::new(None)),
            close_notify: CancellationToken::new(),
        })
    }

    /// Returns the STUN server address.
    fn stun_server_addr(&self) -> String {
        self.stun_serv_addr.clone()
    }

    /// `listen()` will have this client start listening on the `relay_conn` provided via the config.
    /// This is optional. If not used, you will need to call `handle_inbound` method
    /// to supply incoming data, instead.
    async fn listen(&self) -> Result<()> {
        let conn = Arc::clone(&self.conn);
        let stun_serv_str = self.stun_serv_addr.clone();
        let tr_map = Arc::clone(&self.tr_map);
        let read_ch_tx = Arc::clone(&self.read_ch_tx);
        let binding_mgr = Arc::clone(&self.binding_mgr);
        let close_notify = self.close_notify.clone();

        tokio::spawn(async move {
            let mut buf = vec![0u8; MAX_DATA_BUFFER_SIZE];
            let wait_cancel = close_notify.cancelled();
            pin!(wait_cancel);

            loop {
                let (n, from) = select! {
                    biased;

                    _ = &mut wait_cancel => {
                        log::debug!("exiting read loop");
                        break;
                    },
                    result = conn.recv_from(&mut buf) => match result {
                        Ok((n, from)) => (n, from),
                        Err(err) => {
                            log::debug!("exiting read loop: {}", err);
                            break;
                        }
                    }
                };
                log::debug!("received {} bytes of udp from {}", n, from);

                select! {
                    biased;

                    _ = &mut wait_cancel => {
                        log::debug!("exiting read loop");
                        break;
                    },
                    result = ClientInternal::handle_inbound(
                        &read_ch_tx,
                        &buf[..n],
                        from,
                        &stun_serv_str,
                        &tr_map,
                        &binding_mgr,
                    ) => {
                        if let Err(err) = result {
                            log::debug!("exiting read loop: {}", err);
                            break;
                        }
                    }
                }
            }
        });

        Ok(())
    }

    /// Handles data received.
    ///
    /// This method handles incoming packet demultiplex it by the source address
    /// and the types of the message.
    /// Caller should check if the packet was handled by this client or not.
    /// If not handled, it is assumed that the packet is application data.
    /// If an error is returned, the caller should discard the packet regardless.
    async fn handle_inbound(
        read_ch_tx: &Arc<Mutex<Option<mpsc::Sender<InboundData>>>>,
        data: &[u8],
        from: SocketAddr,
        stun_serv_str: &str,
        tr_map: &Arc<Mutex<TransactionMap>>,
        binding_mgr: &Arc<Mutex<BindingManager>>,
    ) -> Result<()> {
        // +-------------------+-------------------------------+
        // |   Return Values   |                               |
        // +-------------------+       Meaning / Action        |
        // | handled |  error  |                               |
        // |=========+=========+===============================+
        // |  false  |   nil   | Handle the packet as app data |
        // |---------+---------+-------------------------------+
        // |  true   |   nil   |        Nothing to do          |
        // |---------+---------+-------------------------------+
        // |  false  |  error  |     (shouldn't happen)        |
        // |---------+---------+-------------------------------+
        // |  true   |  error  | Error occurred while handling |
        // +---------+---------+-------------------------------+
        // Possible causes of the error:
        //  - Malformed packet (parse error)
        //  - STUN message was a request
        //  - Non-STUN message from the STUN server

        if is_message(data) {
            ClientInternal::handle_stun_message(tr_map, read_ch_tx, data, from).await
        } else if ChannelData::is_channel_data(data) {
            ClientInternal::handle_channel_data(binding_mgr, read_ch_tx, data).await
        } else if !stun_serv_str.is_empty() && from.to_string() == *stun_serv_str {
            // received from STUN server but it is not a STUN message
            Err(Error::ErrNonStunmessage)
        } else {
            // assume, this is an application data
            log::trace!("non-STUN/TURN packect, unhandled");
            Ok(())
        }
    }

    async fn handle_stun_message(
        tr_map: &Arc<Mutex<TransactionMap>>,
        read_ch_tx: &Arc<Mutex<Option<mpsc::Sender<InboundData>>>>,
        data: &[u8],
        mut from: SocketAddr,
    ) -> Result<()> {
        let mut msg = Message::new();
        msg.raw = data.to_vec();
        msg.decode()?;

        if msg.typ.class == CLASS_REQUEST {
            return Err(Error::Other(format!(
                "{:?} : {}",
                Error::ErrUnexpectedStunrequestMessage,
                msg
            )));
        }

        if msg.typ.class == CLASS_INDICATION {
            if msg.typ.method == METHOD_DATA {
                let mut peer_addr = PeerAddress::default();
                peer_addr.get_from(&msg)?;
                from = SocketAddr::new(peer_addr.ip, peer_addr.port);

                let mut data = Data::default();
                data.get_from(&msg)?;

                log::debug!("data indication received from {}", from);

                let _ = ClientInternal::handle_inbound_relay_conn(read_ch_tx, &data.0, from).await;
            }

            return Ok(());
        }

        // This is a STUN response message (transactional)
        // The type is either:
        // - stun.ClassSuccessResponse
        // - stun.ClassErrorResponse

        let tr_key = BASE64_STANDARD.encode(msg.transaction_id.0);

        let mut tm = tr_map.lock().await;
        if tm.find(&tr_key).is_none() {
            // silently discard
            log::debug!("no transaction for {}", msg);
            return Ok(());
        }

        if let Some(mut tr) = tm.delete(&tr_key) {
            // End the transaction
            tr.stop_rtx_timer();

            if !tr
                .write_result(TransactionResult {
                    msg,
                    from,
                    retries: tr.retries(),
                    ..Default::default()
                })
                .await
            {
                log::debug!("no listener for msg.raw {:?}", data);
            }
        }

        Ok(())
    }

    async fn handle_channel_data(
        binding_mgr: &Arc<Mutex<BindingManager>>,
        read_ch_tx: &Arc<Mutex<Option<mpsc::Sender<InboundData>>>>,
        data: &[u8],
    ) -> Result<()> {
        let mut ch_data = ChannelData {
            raw: data.to_vec(),
            ..Default::default()
        };
        ch_data.decode()?;

        let addr = ClientInternal::find_addr_by_channel_number(binding_mgr, ch_data.number.0)
            .await
            .ok_or(Error::ErrChannelBindNotFound)?;

        log::trace!(
            "channel data received from {} (ch={})",
            addr,
            ch_data.number.0
        );

        let _ = ClientInternal::handle_inbound_relay_conn(read_ch_tx, &ch_data.data, addr).await;

        Ok(())
    }

    /// Passes inbound data in RelayConn.
    async fn handle_inbound_relay_conn(
        read_ch_tx: &Arc<Mutex<Option<mpsc::Sender<InboundData>>>>,
        data: &[u8],
        from: SocketAddr,
    ) -> Result<()> {
        let read_ch_tx_opt = read_ch_tx.lock().await;
        log::debug!("read_ch_tx_opt = {}", read_ch_tx_opt.is_some());
        if let Some(tx) = &*read_ch_tx_opt {
            log::debug!("try_send data = {:?}, from = {}", data, from);
            if tx
                .try_send(InboundData {
                    data: data.to_vec(),
                    from,
                })
                .is_err()
            {
                log::warn!("receive buffer full");
            }
            Ok(())
        } else {
            Err(Error::ErrAlreadyClosed)
        }
    }

    /// Closes this client.
    async fn close(&mut self) {
        self.close_notify.cancel();
        {
            let mut read_ch_tx = self.read_ch_tx.lock().await;
            read_ch_tx.take();
        }
        {
            let mut tm = self.tr_map.lock().await;
            tm.close_and_delete_all();
        }
    }

    /// Sends a new STUN request to the given transport address.
    async fn send_binding_request_to(&mut self, to: &str) -> Result<SocketAddr> {
        let msg = {
            let attrs: Vec<Box<dyn Setter>> = if !self.software.text.is_empty() {
                vec![
                    Box::new(TransactionId::new()),
                    Box::new(BINDING_REQUEST),
                    Box::new(self.software.clone()),
                ]
            } else {
                vec![Box::new(TransactionId::new()), Box::new(BINDING_REQUEST)]
            };

            let mut msg = Message::new();
            msg.build(&attrs)?;
            msg
        };

        log::debug!("client.SendBindingRequestTo call PerformTransaction 1");
        let tr_res = self.perform_transaction(&msg, to, false).await?;

        let mut refl_addr = XorMappedAddress::default();
        refl_addr.get_from(&tr_res.msg)?;

        Ok(SocketAddr::new(refl_addr.ip, refl_addr.port))
    }

    /// Sends a new STUN request to the STUN server.
    async fn send_binding_request(&mut self) -> Result<SocketAddr> {
        if self.stun_serv_addr.is_empty() {
            Err(Error::ErrStunserverAddressNotSet)
        } else {
            self.send_binding_request_to(&self.stun_serv_addr.clone())
                .await
        }
    }

    /// Returns a peer address associated with the
    // channel number on this UDPConn
    async fn find_addr_by_channel_number(
        binding_mgr: &Arc<Mutex<BindingManager>>,
        ch_num: u16,
    ) -> Option<SocketAddr> {
        let bm = binding_mgr.lock().await;
        bm.find_by_number(ch_num).map(|b| b.addr)
    }

    /// Sends a TURN allocation request to the given transport address.
    async fn allocate(&mut self) -> Result<RelayConnConfig> {
        {
            let read_ch_tx = self.read_ch_tx.lock().await;
            log::debug!("allocate check: read_ch_tx_opt = {}", read_ch_tx.is_some());
            if read_ch_tx.is_some() {
                return Err(Error::ErrOneAllocateOnly);
            }
        }

        let mut msg = Message::new();
        msg.build(&[
            Box::new(TransactionId::new()),
            Box::new(MessageType::new(METHOD_ALLOCATE, CLASS_REQUEST)),
            Box::new(RequestedTransport {
                protocol: PROTO_UDP,
            }),
            Box::new(FINGERPRINT),
        ])?;

        log::debug!("client.Allocate call PerformTransaction 1");
        let tr_res = self
            .perform_transaction(&msg, &self.turn_serv_addr.clone(), false)
            .await?;
        let res = tr_res.msg;

        // Anonymous allocate failed, trying to authenticate.
        let nonce = Nonce::get_from_as(&res, ATTR_NONCE)?;
        self.realm = Realm::get_from_as(&res, ATTR_REALM)?;

        self.integrity = MessageIntegrity::new_long_term_integrity(
            self.username.text.clone(),
            self.realm.text.clone(),
            self.password.clone(),
        );

        // Trying to authorize.
        msg.build(&[
            Box::new(TransactionId::new()),
            Box::new(MessageType::new(METHOD_ALLOCATE, CLASS_REQUEST)),
            Box::new(RequestedTransport {
                protocol: PROTO_UDP,
            }),
            Box::new(self.username.clone()),
            Box::new(self.realm.clone()),
            Box::new(nonce.clone()),
            Box::new(self.integrity.clone()),
            Box::new(FINGERPRINT),
        ])?;

        log::debug!("client.Allocate call PerformTransaction 2");
        let tr_res = self
            .perform_transaction(&msg, &self.turn_serv_addr.clone(), false)
            .await?;
        let res = tr_res.msg;

        if res.typ.class == CLASS_ERROR_RESPONSE {
            let mut code = ErrorCodeAttribute::default();
            let result = code.get_from(&res);
            if result.is_err() {
                return Err(Error::Other(format!("{}", res.typ)));
            } else {
                return Err(Error::Other(format!("{} (error {})", res.typ, code)));
            }
        }

        // Getting relayed addresses from response.
        let mut relayed = RelayedAddress::default();
        relayed.get_from(&res)?;
        let relayed_addr = SocketAddr::new(relayed.ip, relayed.port);

        // Getting lifetime from response
        let mut lifetime = Lifetime::default();
        lifetime.get_from(&res)?;

        let (read_ch_tx, read_ch_rx) = mpsc::channel(MAX_READ_QUEUE_SIZE);
        {
            let mut read_ch_tx_opt = self.read_ch_tx.lock().await;
            *read_ch_tx_opt = Some(read_ch_tx);
            log::debug!("allocate: read_ch_tx_opt = {}", read_ch_tx_opt.is_some());
        }

        Ok(RelayConnConfig {
            relayed_addr,
            integrity: self.integrity.clone(),
            nonce,
            lifetime: lifetime.0,
            binding_mgr: Arc::clone(&self.binding_mgr),
            read_ch_rx: Arc::new(Mutex::new(read_ch_rx)),
        })
    }
}

/// Client is a STUN server client.
#[derive(Clone)]
pub struct Client {
    client_internal: Arc<Mutex<ClientInternal>>,
}

impl Client {
    pub async fn new(config: ClientConfig) -> Result<Self> {
        let ci = ClientInternal::new(config).await?;
        Ok(Client {
            client_internal: Arc::new(Mutex::new(ci)),
        })
    }

    pub async fn listen(&self) -> Result<()> {
        let ci = self.client_internal.lock().await;
        ci.listen().await
    }

    pub async fn allocate(&self) -> Result<impl Conn> {
        let config = {
            let mut ci = self.client_internal.lock().await;
            ci.allocate().await?
        };

        Ok(RelayConn::new(Arc::clone(&self.client_internal), config).await)
    }

    pub async fn close(&self) -> Result<()> {
        let mut ci = self.client_internal.lock().await;
        ci.close().await;
        Ok(())
    }

    /// Sends a new STUN request to the given transport address.
    pub async fn send_binding_request_to(&self, to: &str) -> Result<SocketAddr> {
        let mut ci = self.client_internal.lock().await;
        ci.send_binding_request_to(to).await
    }

    /// Sends a new STUN request to the STUN server.
    pub async fn send_binding_request(&self) -> Result<SocketAddr> {
        let mut ci = self.client_internal.lock().await;
        ci.send_binding_request().await
    }
}
