pub mod binding;
pub mod conn;
pub mod periodic_timer;
pub mod permission;
pub mod transaction;

//use binding::*;
//use conn::*;
//use periodic_timer::*;
//use permission::*;
use crate::errors::*;
use crate::proto::chandata::*;
use crate::proto::peeraddr::*;
use transaction::*;

use stun::attributes::*;
use stun::integrity::*;
use stun::message::*;
use stun::textattrs::*;

use std::sync::Arc;

use tokio::net::{ToSocketAddrs, UdpSocket};
use tokio::time::Duration;

use std::net::SocketAddr;
use tokio::sync::Mutex;
use util::Error;

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
    stun_serv_addr: String, // STUN server address (e.g. "stun.abc.com:3478")
    turn_serv_addr: String, // TURN server addrees (e.g. "turn.abc.com:3478")
    username: String,
    password: String,
    realm: String,
    software: String,
    rto: Duration,
    conn: Arc<UdpSocket>, // Listening socket (net.PacketConn)
}

// Client is a STUN server client
pub struct Client {
    conn: Arc<UdpSocket>,        // read-only
    stun_serv_addr: String,      // read-only, used for dmuxing
    turn_serv_addr: String,      // read-only, used for dmuxing
    username: Username,          // read-only
    password: String,            // read-only
    realm: Realm,                // read-only
    integrity: MessageIntegrity, // read-only
    software: Software,          // read-only
    tr_map: TransactionMap,      // thread-safe
    rto: Duration,               // read-only
                                 //relayedConn   *client.UDPConn        // protected by mutex ***
                                 //allocTryLock  client.TryLock         // thread-safe
                                 //listenTryLock client.TryLock         // thread-safe
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

    // turn_server_addr return the TURN server address
    pub fn turn_server_addr(&self) -> String {
        self.turn_serv_addr.clone()
    }

    // stun_server_addr return the STUN server address
    pub fn stun_server_addr(&self) -> String {
        self.stun_serv_addr.clone()
    }

    // username returns username
    pub fn username(&self) -> Username {
        self.username.clone()
    }

    // realm return realm
    pub fn realm(&self) -> Realm {
        self.realm.clone()
    }

    // WriteTo sends data to the specified destination using the base socket.
    pub async fn write_to<A: ToSocketAddrs>(&self, data: &[u8], to: A) -> Result<usize, Error> {
        let n = self.conn.send_to(data, to).await?;
        Ok(n)
    }

    // Listen will have this client start listening on the conn provided via the config.
    // This is optional. If not used, you will need to call handle_inbound method
    // to supply incoming data, instead.
    pub async fn listen(conn: Arc<UdpSocket>) -> Result<(), Error> {
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

                if let Err(err) = Client::handle_inbound(&buf[..n], from) {
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
    pub fn handle_inbound(_data: &[u8], _from: SocketAddr) -> Result<bool, Error> {
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

        /*switch {
        case stun.IsMessage(data):
            return true, c.handle_stunmessage(data, from)
        case proto.IsChannelData(data):
            return true, c.handle_channel_data(data)
        case len(c.stunServStr) != 0 && from.String() == c.stunServStr:
            // received from STUN server but it is not a STUN message
            return true, errNonSTUNMessage
        default:
            // assume, this is an application data
            c.log.Tracef("non-STUN/TURN packect, unhandled")
        }*/

        Ok(false)
    }

    async fn handle_stun_message(
        tr_map: &Arc<Mutex<TransactionMap>>,
        data: &[u8],
        _from: SocketAddr,
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
                /*TODO: from = &net.UDPAddr{
                    IP:   peer_addr.IP,
                    Port: peer_addr.Port,
                }

                var data proto.Data
                if err := data.GetFrom(msg); err != nil {
                    return err
                }

                c.log.Debugf("data indication received from %s", from.String())

                relayedConn := c.relayedUDPConn()
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
        if let Some(_tr) = tm.find(&tr_key) {
            // End the transaction
            //TODO: tr.StopRtxTimer()
            tm.delete(&tr_key);

        /*TODO: if !tr.WriteResult(client.TransactionResult{
            Msg:     msg,
            From:    from,
            Retries: tr.Retries(),
        }) {
            c.log.Debugf("no listener for %s", msg.String())
        }*/
        } else {
            // silently discard
            log::debug!("no transaction for {}", msg);
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
        //TODO: self.tr_map.CloseAndDeleteAll()
    }
}

/*


// TransactionID & Base64: https://play.golang.org/p/EEgmJDI971P

// SendBindingRequestTo sends a new STUN request to the given transport address
func (c *Client) SendBindingRequestTo(to net.Addr) (net.Addr, error) {
    attrs := []stun.Setter{stun.TransactionID, stun.BindingRequest}
    if len(c.software) > 0 {
        attrs = append(attrs, c.software)
    }

    msg, err := stun.Build(attrs...)
    if err != nil {
        return nil, err
    }
    trRes, err := c.PerformTransaction(msg, to, false)
    if err != nil {
        return nil, err
    }

    var reflAddr stun.XORMappedAddress
    if err := reflAddr.GetFrom(trRes.Msg); err != nil {
        return nil, err
    }

    return &net.UDPAddr{
        IP:   reflAddr.IP,
        Port: reflAddr.Port,
    }, nil
}

// SendBindingRequest sends a new STUN request to the STUN server
func (c *Client) SendBindingRequest() (net.Addr, error) {
    if c.stun_serv == nil {
        return nil, errSTUNServerAddressNotSet
    }
    return c.SendBindingRequestTo(c.stun_serv)
}

// Allocate sends a TURN allocation request to the given transport address
func (c *Client) Allocate() (net.PacketConn, error) {
    if err := c.allocTryLock.Lock(); err != nil {
        return nil, fmt.Errorf("%w: %s", errOneAllocateOnly, err.Error())
    }
    defer c.allocTryLock.Unlock()

    relayedConn := c.relayedUDPConn()
    if relayedConn != nil {
        return nil, fmt.Errorf("%w: %s", errAlreadyAllocated, relayedConn.LocalAddr().String())
    }

    msg, err := stun.Build(
        stun.TransactionID,
        stun.NewType(stun.MethodAllocate, stun.ClassRequest),
        proto.RequestedTransport{Protocol: proto.ProtoUDP},
        stun.Fingerprint,
    )
    if err != nil {
        return nil, err
    }

    trRes, err := c.PerformTransaction(msg, c.turn_serv, false)
    if err != nil {
        return nil, err
    }

    res := trRes.Msg

    // Anonymous allocate failed, trying to authenticate.
    var nonce stun.Nonce
    if err = nonce.GetFrom(res); err != nil {
        return nil, err
    }
    if err = c.realm.GetFrom(res); err != nil {
        return nil, err
    }
    c.realm = append([]byte(nil), c.realm...)
    c.integrity = stun.NewLongTermIntegrity(
        c.username.String(), c.realm.String(), c.password,
    )
    // Trying to authorize.
    msg, err = stun.Build(
        stun.TransactionID,
        stun.NewType(stun.MethodAllocate, stun.ClassRequest),
        proto.RequestedTransport{Protocol: proto.ProtoUDP},
        &c.username,
        &c.realm,
        &nonce,
        &c.integrity,
        stun.Fingerprint,
    )
    if err != nil {
        return nil, err
    }

    trRes, err = c.PerformTransaction(msg, c.turn_serv, false)
    if err != nil {
        return nil, err
    }
    res = trRes.Msg

    if res.Type.Class == stun.ClassErrorResponse {
        var code stun.ErrorCodeAttribute
        if err = code.GetFrom(res); err == nil {
            return nil, fmt.Errorf("%s (error %s)", res.Type, code) //nolint:goerr113
        }
        return nil, fmt.Errorf("%s", res.Type) //nolint:goerr113
    }

    // Getting relayed addresses from response.
    var relayed proto.RelayedAddress
    if err := relayed.GetFrom(res); err != nil {
        return nil, err
    }
    relayedAddr := &net.UDPAddr{
        IP:   relayed.IP,
        Port: relayed.Port,
    }

    // Getting lifetime from response
    var lifetime proto.Lifetime
    if err := lifetime.GetFrom(res); err != nil {
        return nil, err
    }

    relayedConn = client.NewUDPConn(&client.UDPConnConfig{
        Observer:    c,
        RelayedAddr: relayedAddr,
        Integrity:   c.integrity,
        Nonce:       nonce,
        Lifetime:    lifetime.Duration,
        Log:         c.log,
    })

    c.setRelayedUDPConn(relayedConn)

    return relayedConn, nil
}

// PerformTransaction performs STUN transaction
func (c *Client) PerformTransaction(msg *stun.Message, to net.Addr, ignoreResult bool) (client.TransactionResult,
    error) {
    trKey := b64.StdEncoding.EncodeToString(msg.TransactionID[:])

    raw := make([]byte, len(msg.Raw))
    copy(raw, msg.Raw)

    tr := client.NewTransaction(&client.TransactionConfig{
        Key:          trKey,
        Raw:          raw,
        To:           to,
        Interval:     c.rto,
        IgnoreResult: ignoreResult,
    })

    c.tr_map.Insert(trKey, tr)

    c.log.Tracef("start %s transaction %s to %s", msg.Type, trKey, tr.To.String())
    _, err := c.conn.WriteTo(tr.Raw, to)
    if err != nil {
        return client.TransactionResult{}, err
    }

    tr.StartRtxTimer(c.on_rtx_timeout)

    // If dontWait is true, get the transaction going and return immediately
    if ignoreResult {
        return client.TransactionResult{}, nil
    }

    res := tr.WaitForResult()
    if res.Err != nil {
        return res, res.Err
    }
    return res, nil
}

// OnDeallocated is called when deallocation of relay address has been complete.
// (Called by UDPConn)
func (c *Client) OnDeallocated(relayedAddr net.Addr) {
    c.setRelayedUDPConn(nil)
}


func (c *Client) on_rtx_timeout(trKey string, nRtx int) {
    c.mutexTrMap.Lock()
    defer c.mutexTrMap.Unlock()

    tr, ok := c.tr_map.Find(trKey)
    if !ok {
        return // already gone
    }

    if nRtx == MAX_RTX_COUNT {
        // all retransmisstions failed
        c.tr_map.Delete(trKey)
        if !tr.WriteResult(client.TransactionResult{
            Err: fmt.Errorf("%w %s", errAllRetransmissionsFailed, trKey),
        }) {
            c.log.Debug("no listener for transaction")
        }
        return
    }

    c.log.Tracef("retransmitting transaction %s to %s (nRtx=%d)",
        trKey, tr.To.String(), nRtx)
    _, err := c.conn.WriteTo(tr.Raw, tr.To)
    if err != nil {
        c.tr_map.Delete(trKey)
        if !tr.WriteResult(client.TransactionResult{
            Err: fmt.Errorf("%w %s", errFailedToRetransmitTransaction, trKey),
        }) {
            c.log.Debug("no listener for transaction")
        }
        return
    }
    tr.StartRtxTimer(c.on_rtx_timeout)
}

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
