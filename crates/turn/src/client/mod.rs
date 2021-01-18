pub mod binding;
pub mod conn;
pub mod periodic_timer;
pub mod permission;
pub mod transaction;

use crate::errors::*;
use crate::proto::{
    chandata::*, data::*, lifetime::*, peeraddr::*, relayaddr::*, reqtrans::*, PROTO_UDP,
};
use conn::*;
use transaction::*;

use stun::agent::*;
use stun::attributes::*;
use stun::error_code::*;
use stun::fingerprint::*;
use stun::integrity::*;
use stun::message::*;
use stun::textattrs::*;
use stun::xoraddr::*;

use std::sync::Arc;

use tokio::net::UdpSocket;
use tokio::time::Duration;

use std::net::SocketAddr;
use tokio::sync::Mutex;
use util::Error;

use async_trait::async_trait;

const DEFAULT_RTO: Duration = Duration::from_millis(200);
const MAX_RTX_COUNT: u16 = 7; // total 7 requests (Rc)
const MAX_DATA_BUFFER_SIZE: usize = u16::MAX as usize; // message size limit for Chromium

//              interval [msec]
// 0: 0 ms      +500
// 1: 500 ms	+1000
// 2: 1500 ms   +2000
// 3: 3500 ms   +4000
// 4: 7500 ms   +8000
// 5: 15500 ms  +16000
// 6: 31500 ms  +32000
// -: 63500 ms  failed

// ClientConfig is a bag of config parameters for Client.
pub struct ClientConfig {
    stun_serv_addr: Option<SocketAddr>, // STUN server address (e.g. "stun.abc.com:3478")
    turn_serv_addr: SocketAddr,         // TURN server addrees (e.g. "turn.abc.com:3478")
    username: String,
    password: String,
    realm: String,
    software: String,
    rto: Duration,
    conn: Arc<UdpSocket>, // Listening socket (net.PacketConn)
}

// Client is a STUN server client
pub struct Client {
    conn: Arc<UdpSocket>,
    stun_serv_addr: Option<SocketAddr>,
    turn_serv_addr: SocketAddr,
    username: Username,
    password: String,
    realm: Realm,
    integrity: MessageIntegrity,
    software: Software,
    tr_map: TransactionMap,
    rto: Duration,
    //relayedConn   *client.UDPConn
}

#[async_trait]
impl RelayConnObserver for Client {
    // turn_server_addr return the TURN server address
    fn turn_server_addr(&self) -> SocketAddr {
        self.turn_serv_addr
    }

    // username returns username
    fn username(&self) -> Username {
        self.username.clone()
    }

    // realm return realm
    fn realm(&self) -> Realm {
        self.realm.clone()
    }

    // WriteTo sends data to the specified destination using the base socket.
    async fn write_to(&self, data: &[u8], to: SocketAddr) -> Result<usize, Error> {
        let n = self.conn.send_to(data, to).await?;
        Ok(n)
    }

    // PerformTransaction performs STUN transaction
    async fn perform_transaction(
        &mut self,
        msg: &Message,
        to: SocketAddr,
        ignore_result: bool,
    ) -> Result<TransactionResult, Error> {
        let tr_key = base64::encode(&msg.transaction_id.0);

        let tr = Transaction::new(TransactionConfig {
            key: tr_key.clone(),
            raw: msg.raw.clone(),
            to,
            interval: self.rto,
            ignore_result,
        });

        log::trace!("start {} transaction {} to {}", msg.typ, tr_key, tr.to);
        self.tr_map.insert(tr_key, tr);

        self.conn.send_to(&msg.raw, to).await?;

        //TODO: tr.start_rtx_timer(c.on_rtx_timeout)

        // If dontWait is true, get the transaction going and return immediately
        if ignore_result {
            return Ok(TransactionResult::default());
        }

        //TODO: tr.wait_for_result().await
        Ok(TransactionResult::default())
    }

    // OnDeallocated is called when deallocation of relay address has been complete.
    // (Called by UDPConn)
    async fn on_deallocated(&self, _relayed_ddr: SocketAddr) {
        //TODO: c.setRelayedUDPConn(nil)
    }
}

impl Client {
    // new returns a new Client instance. listeningAddress is the address and port to listen on, default "0.0.0.0:0"
    pub fn new(config: ClientConfig) -> Self {
        Client {
            conn: Arc::clone(&config.conn),
            stun_serv_addr: config.stun_serv_addr,
            turn_serv_addr: config.turn_serv_addr,
            username: Username::new(ATTR_USERNAME, config.username),
            password: config.password,
            realm: Realm::new(ATTR_REALM, config.realm),
            software: Software::new(ATTR_SOFTWARE, config.software),
            tr_map: TransactionMap::new(),
            rto: if config.rto > Duration::from_secs(0) {
                config.rto
            } else {
                DEFAULT_RTO
            },
            integrity: MessageIntegrity::new_short_term_integrity(String::new()),
        }
    }

    // stun_server_addr return the STUN server address
    pub fn stun_server_addr(&self) -> Option<SocketAddr> {
        self.stun_serv_addr
    }

    // Listen will have this client start listening on the conn provided via the config.
    // This is optional. If not used, you will need to call handle_inbound method
    // to supply incoming data, instead.
    pub async fn listen(
        conn: Arc<UdpSocket>,
        stun_serv_str: Option<SocketAddr>,
        tr_map: Arc<Mutex<TransactionMap>>,
    ) -> Result<(), Error> {
        tokio::spawn(async move {
            let mut buf = vec![0u8; MAX_DATA_BUFFER_SIZE];
            loop {
                let (n, from) = match conn.recv_from(&mut buf).await {
                    Ok((n, from)) => (n, from),
                    Err(err) => {
                        log::debug!("exiting read loop: {}", err);
                        break;
                    }
                };

                if let Err(err) =
                    Client::handle_inbound(&buf[..n], from, stun_serv_str, &tr_map).await
                {
                    log::debug!("exiting read loop: {}", err);
                    break;
                }
            }
        });

        Ok(())
    }

    // handle_inbound handles data received.
    // This method handles incoming packet demultiplex it by the source address
    // and the types of the message.
    // This return a booleen (handled or not) and if there was an error.
    // Caller should check if the packet was handled by this client or not.
    // If not handled, it is assumed that the packet is application data.
    // If an error is returned, the caller should discard the packet regardless.
    async fn handle_inbound(
        data: &[u8],
        from: SocketAddr,
        stun_serv_str: Option<SocketAddr>,
        tr_map: &Arc<Mutex<TransactionMap>>,
    ) -> Result<(), Error> {
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
            Client::handle_stun_message(tr_map, data, from).await
        } else if ChannelData::is_channel_data(data) {
            Client::handle_channel_data(data).await
        } else if stun_serv_str.is_some() && Some(from) == stun_serv_str {
            // received from STUN server but it is not a STUN message
            Err(ERR_NON_STUNMESSAGE.to_owned())
        } else {
            // assume, this is an application data
            log::trace!("non-STUN/TURN packect, unhandled");
            Ok(())
        }
    }

    async fn handle_stun_message(
        tr_map: &Arc<Mutex<TransactionMap>>,
        data: &[u8],
        mut from: SocketAddr,
    ) -> Result<(), Error> {
        let mut msg = Message::new();
        msg.raw = data.to_vec();
        msg.decode()?;

        if msg.typ.class == CLASS_REQUEST {
            return Err(Error::new(format!(
                "{} : {}",
                *ERR_UNEXPECTED_STUNREQUEST_MESSAGE, msg
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

                /*TODO: relayedConn := c.relayedUDPConn()
                if relayedConn == nil {
                    c.log.Debug("no relayed conn allocated")
                    return nil // silently discard
                }

                relayedConn.handle_inbound(data, from)*/
            }

            return Ok(());
        }

        // This is a STUN response message (transactional)
        // The type is either:
        // - stun.ClassSuccessResponse
        // - stun.ClassErrorResponse

        let tr_key = base64::encode(&msg.transaction_id.0);

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

    async fn handle_channel_data(data: &[u8]) -> Result<(), Error> {
        let mut ch_data = ChannelData {
            raw: data.to_vec(),
            ..Default::default()
        };
        ch_data.decode()?;

        /*TODO: relayedConn := c.relayedUDPConn()
        if relayedConn == nil {
            c.log.Debug("no relayed conn allocated")
            return nil // silently discard
        }

        addr, ok := relayedConn.find_addr_by_channel_number(uint16(ch_data.Number))
        if !ok {
            return fmt.Errorf("%w: %d", errChannelBindNotFound, int(ch_data.Number))
        }

        c.log.Tracef("channel data received from %s (ch=%d)", addr.String(), int(ch_data.Number))

        relayedConn.handle_inbound(ch_data.Data, addr)
         */

        Ok(())
    }

    // Close closes this client
    pub fn close(&mut self) {
        self.tr_map.close_and_delete_all();
    }

    // send_binding_request_to sends a new STUN request to the given transport address
    pub async fn send_binding_request_to(&mut self, to: SocketAddr) -> Result<SocketAddr, Error> {
        let mut attrs: Vec<Box<dyn Setter>> =
            vec![Box::new(TransactionId::new()), Box::new(BINDING_REQUEST)];
        if !self.software.text.is_empty() {
            attrs.push(Box::new(self.software.clone()));
        }

        let mut msg = Message::new();
        msg.build(&attrs)?;

        let tr_res = self.perform_transaction(&msg, to, false).await?;

        let mut refl_addr = XORMappedAddress::default();
        refl_addr.get_from(&tr_res.msg)?;

        Ok(SocketAddr::new(refl_addr.ip, refl_addr.port))
    }

    // send_binding_request sends a new STUN request to the STUN server
    pub async fn send_binding_request(&mut self) -> Result<SocketAddr, Error> {
        if let Some(stun_serv_addr) = self.stun_serv_addr {
            self.send_binding_request_to(stun_serv_addr).await
        } else {
            Err(ERR_STUNSERVER_ADDRESS_NOT_SET.to_owned())
        }
    }

    // Allocate sends a TURN allocation request to the given transport address
    pub async fn allocate(&mut self) -> Result<SocketAddr, Error> {
        /*TODO: relayedConn := c.relayedUDPConn()
        if relayedConn != nil {
            return nil, fmt.Errorf("%w: %s", errAlreadyAllocated, relayedConn.LocalAddr().String())
        }*/

        let mut msg = Message::new();
        msg.build(&[
            Box::new(TransactionId::new()),
            Box::new(MessageType::new(METHOD_ALLOCATE, CLASS_REQUEST)),
            Box::new(RequestedTransport {
                protocol: PROTO_UDP,
            }),
            Box::new(FINGERPRINT),
        ])?;

        let tr_res = self
            .perform_transaction(&msg, self.turn_serv_addr, false)
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
            Box::new(nonce),
            Box::new(self.integrity.clone()),
            Box::new(FINGERPRINT),
        ])?;

        let tr_res = self
            .perform_transaction(&msg, self.turn_serv_addr, false)
            .await?;
        let res = tr_res.msg;

        if res.typ.class == CLASS_ERROR_RESPONSE {
            let mut code = ErrorCodeAttribute::default();
            let result = code.get_from(&res);
            if result.is_err() {
                return Err(Error::new(format!("{}", res.typ)));
            } else {
                return Err(Error::new(format!("{} (error {})", res.typ, code)));
            }
        }

        // Getting relayed addresses from response.
        let mut relayed = RelayedAddress::default();
        relayed.get_from(&res)?;
        let relayed_addr = SocketAddr::new(relayed.ip, relayed.port);

        // Getting lifetime from response
        let mut lifetime = Lifetime::default();
        lifetime.get_from(&res)?;

        /*TODO: relayedConn = client.NewUDPConn(&client.UDPConnConfig{
            Observer:    c,
            RelayedAddr: relayed_addr,
            Integrity:   c.integrity,
            Nonce:       nonce,
            Lifetime:    lifetime.Duration,
            Log:         c.log,
        })

        c.setRelayedUDPConn(relayedConn)

        return relayedConn, nil

         */
        Ok(relayed_addr)
    }

    pub async fn on_rtx_timeout(&mut self, tr_key: String, n_rtx: u16) {
        let (tr_raw, tr_to) = match self.tr_map.find(&tr_key) {
            Some(tr) => (tr.raw.clone(), tr.to),
            None => return, // already gone
        };

        if n_rtx == MAX_RTX_COUNT {
            // all retransmisstions failed
            if let Some(mut tr) = self.tr_map.delete(&tr_key) {
                if !tr
                    .write_result(TransactionResult {
                        err: Some(Error::new(format!(
                            "{} {}",
                            *ERR_ALL_RETRANSMISSIONS_FAILED, tr_key
                        ))),
                        ..Default::default()
                    })
                    .await
                {
                    log::debug!("no listener for transaction");
                }
            }
            return;
        }

        log::trace!(
            "retransmitting transaction {} to {} (n_rtx={})",
            tr_key,
            tr_to,
            n_rtx
        );

        if self.conn.send_to(&tr_raw, tr_to).await.is_err() {
            if let Some(mut tr) = self.tr_map.delete(&tr_key) {
                if !tr
                    .write_result(TransactionResult {
                        err: Some(Error::new(format!(
                            "{} {}",
                            *ERR_ALL_RETRANSMISSIONS_FAILED, tr_key
                        ))),
                        ..Default::default()
                    })
                    .await
                {
                    log::debug!("no listener for transaction");
                }
            }
            return;
        }
        //tr.start_rtx_timer(.on_rtx_timeout)
    }
}

/*
func (c *Client) setRelayedUDPConn(conn *client.UDPConn) {
    c.mutex.Lock()
    defer c.mutex.Unlock()

    c.relayedConn = conn
}

func (c *Client) relayedUDPConn() *client.UDPConn {
    c.mutex.RLock()
    defer c.mutex.RUnlock()

    return c.relayedConn
}
*/
