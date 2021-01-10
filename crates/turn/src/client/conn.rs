// client implements the API for a TURN client
use super::periodic_timer::*;
use super::permission::*;
use super::transaction::*;
//use crate::proto::*;

use stun::integrity::*;
use stun::message::*;
use stun::textattrs::*;

use util::Error;

use std::net::SocketAddr;

use tokio::sync::mpsc;
use tokio::time::{Duration, Sleep};

const MAX_READ_QUEUE_SIZE: usize = 1024;
const PERM_REFRESH_INTERVAL: Duration = Duration::from_secs(120);
const MAX_RETRY_ATTEMPTS: u16 = 3;

enum TimerIdRefresh {
    Alloc,
    Perms,
}

/*
func noDeadline() time.Time {
    return time.Time{}
}
*/

struct InboundData {
    data: Vec<u8>,
    from: SocketAddr,
}

// UDPConnObserver is an interface to UDPConn observer
pub trait UDPConnObserver {
    fn turnserver_addr(&self) -> SocketAddr;
    fn username(&self) -> TextAttribute;
    fn realm(&self) -> TextAttribute;
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
    nonce: TextAttribute,
    lifetime: Duration,
    //Log         :logging.LeveledLogger,
}

// UDPConn is the implementation of the Conn and PacketConn interfaces for UDP network connections.
// compatible with net.PacketConn and net.Conn
pub struct UDPConn {
    obs: Box<dyn UDPConnObserver>, // read-only
    relayed_addr: SocketAddr,      // read-only
    perm_map: PermissionMap,       // thread-safe
    //TODO: bindingMgr        :*bindingManager,       // thread-safe
    integrity: MessageIntegrity,          // read-only
    nonce: TextAttribute,                 // needs mutex x
    lifetime: Duration,                   // needs mutex x
    read_ch: mpsc::Receiver<InboundData>, // thread-safe
    close_ch: mpsc::Receiver<()>,         // thread-safe
    read_timer: Sleep,                    // thread-safe
    refresh_alloc_timer: PeriodicTimer,   // thread-safe
    refresh_perms_timer: PeriodicTimer,   // thread-safe
                                          //mutex             :sync.RWMutex          // thread-safe
                                          //log               :logging.LeveledLogger // read-only
}
/*
// NewUDPConn creates a new instance of UDPConn
func NewUDPConn(config *UDPConnConfig) *UDPConn {
    c := &UDPConn{
        obs:         config.observer,
        relayed_addr: config.relayed_addr,
        perm_map:     newPermissionMap(),
        bindingMgr:  newBindingManager(),
        integrity:   config.integrity,
        _nonce:      config.nonce,
        _lifetime:   config.lifetime,
        read_ch:      make(chan *InboundData, MAX_READ_QUEUE_SIZE),
        close_ch:     make(chan struct{}),
        read_timer:   time.NewTimer(time.Duration(math.MaxInt64)),
        log:         config.Log,
    }

    c.log.Debugf("initial lifetime: %d seconds", int(c.lifetime().Seconds()))

    c.refresh_alloc_timer = NewPeriodicTimer(
        timerIDRefreshAlloc,
        c.onRefreshTimers,
        c.lifetime()/2,
    )

    c.refresh_perms_timer = NewPeriodicTimer(
        timerIDRefreshPerms,
        c.onRefreshTimers,
        PERM_REFRESH_INTERVAL,
    )

    if c.refresh_alloc_timer.Start() {
        c.log.Debugf("refresh_alloc_timer started")
    }
    if c.refresh_perms_timer.Start() {
        c.log.Debugf("refresh_perms_timer started")
    }

    return c
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
func (c *UDPConn) ReadFrom(p []byte) (n int, addr net.Addr, err error) {
    for {
        select {
        case ibData := <-c.read_ch:
            n := copy(p, ibData.data)
            if n < len(ibData.data) {
                return 0, nil, io.ErrShortBuffer
            }
            return n, ibData.from, nil

        case <-c.read_timer.C:
            return 0, nil, &net.OpError{
                Op:   "read",
                Net:  c.LocalAddr().Network(),
                Addr: c.LocalAddr(),
                Err:  newTimeoutError("i/o timeout"),
            }

        case <-c.close_ch:
            return 0, nil, &net.OpError{
                Op:   "read",
                Net:  c.LocalAddr().Network(),
                Addr: c.LocalAddr(),
                Err:  errClosed,
            }
        }
    }
}

// write_to writes a packet with payload p to addr.
// write_to can be made to time out and return
// an Error with Timeout() == true after a fixed time limit;
// see SetDeadline and SetWriteDeadline.
// On packet-oriented connections, write timeouts are rare.
func (c *UDPConn) write_to(p []byte, addr net.Addr) (int, error) { //nolint: gocognit
    var err error
    _, ok := addr.(*net.UDPAddr)
    if !ok {
        return 0, errUDPAddrCast
    }

    // check if we have a permission for the destination IP addr
    perm, ok := c.perm_map.find(addr)
    if !ok {
        perm = &permission{}
        c.perm_map.insert(addr, perm)
    }

    // This func-block would block, per destination IP (, or perm), until
    // the perm state becomes "requested". Purpose of this is to guarantee
    // the order of packets (within the same perm).
    // Note that CreatePermission transaction may not be complete before
    // all the data transmission. This is done assuming that the request
    // will be mostly likely successful and we can tolerate some loss of
    // UDP packet (or reorder), inorder to minimize the latency in most cases.
    createPermission := func() error {
        perm.mutex.Lock()
        defer perm.mutex.Unlock()

        if perm.state() == permStateIdle {
            // punch a hole! (this would block a bit..)
            if err = c.createPermissions(addr); err != nil {
                c.perm_map.delete(addr)
                return err
            }
            perm.setState(permStatePermitted)
        }
        return nil
    }

    for i := 0; i < MAX_RETRY_ATTEMPTS; i++ {
        if err = createPermission(); !errors.Is(err, errTryAgain) {
            break
        }
    }
    if err != nil {
        return 0, err
    }

    // bind channel
    b, ok := c.bindingMgr.findByAddr(addr)
    if !ok {
        b = c.bindingMgr.create(addr)
    }

    bindSt := b.state()

    if bindSt == bindingStateIdle || bindSt == bindingStateRequest || bindSt == bindingStateFailed {
        func() {
            // block only callers with the same binding until
            // the binding transaction has been complete
            b.muBind.Lock()
            defer b.muBind.Unlock()

            // binding state may have been changed while waiting. check again.
            if b.state() == bindingStateIdle {
                b.setState(bindingStateRequest)
                go func() {
                    err2 := c.bind(b)
                    if err2 != nil {
                        c.log.Warnf("bind() failed: %s", err2.Error())
                        b.setState(bindingStateFailed)
                        // keep going...
                    } else {
                        b.setState(bindingStateReady)
                    }
                }()
            }
        }()

        // send data using SendIndication
        peerAddr := addr2PeerAddress(addr)
        var msg *stun.Message
        msg, err = stun.Build(
            stun.TransactionID,
            stun.NewType(stun.MethodSend, stun.ClassIndication),
            proto.Data(p),
            peerAddr,
            stun.Fingerprint,
        )
        if err != nil {
            return 0, err
        }

        // indication has no transaction (fire-and-forget)

        return c.obs.write_to(msg.Raw, c.obs.turnserver_addr())
    }

    // binding is either ready

    // check if the binding needs a refresh
    func() {
        b.muBind.Lock()
        defer b.muBind.Unlock()

        if b.state() == bindingStateReady && time.Since(b.refreshedAt()) > 5*time.Minute {
            b.setState(bindingStateRefresh)
            go func() {
                err = c.bind(b)
                if err != nil {
                    c.log.Warnf("bind() for refresh failed: %s", err.Error())
                    b.setState(bindingStateFailed)
                    // keep going...
                } else {
                    b.setRefreshedAt(time.Now())
                    b.setState(bindingStateReady)
                }
            }()
        }
    }()

    // send via ChannelData
    return c.sendChannelData(p, b.number)
}

// Close closes the connection.
// Any blocked ReadFrom or write_to operations will be unblocked and return errors.
func (c *UDPConn) Close() error {
    c.refresh_alloc_timer.Stop()
    c.refresh_perms_timer.Stop()

    select {
    case <-c.close_ch:
        return errAlreadyClosed
    default:
        close(c.close_ch)
    }

    c.obs.on_deallocated(c.relayed_addr)
    return c.refreshAllocation(0, true /* dontWait=true */)
}

// LocalAddr returns the local network address.
func (c *UDPConn) LocalAddr() net.Addr {
    return c.relayed_addr
}

// SetDeadline sets the read and write deadlines associated
// with the connection. It is equivalent to calling both
// SetReadDeadline and SetWriteDeadline.
//
// A deadline is an absolute time after which I/O operations
// fail with a timeout (see type Error) instead of
// blocking. The deadline applies to all future and pending
// I/O, not just the immediately following call to ReadFrom or
// write_to. After a deadline has been exceeded, the connection
// can be refreshed by setting a deadline in the future.
//
// An idle timeout can be implemented by repeatedly extending
// the deadline after successful ReadFrom or write_to calls.
//
// A zero value for t means I/O operations will not time out.
func (c *UDPConn) SetDeadline(t time.Time) error {
    return c.SetReadDeadline(t)
}

// SetReadDeadline sets the deadline for future ReadFrom calls
// and any currently-blocked ReadFrom call.
// A zero value for t means ReadFrom will not time out.
func (c *UDPConn) SetReadDeadline(t time.Time) error {
    var d time.Duration
    if t == noDeadline() {
        d = time.Duration(math.MaxInt64)
    } else {
        d = time.Until(t)
    }
    c.read_timer.Reset(d)
    return nil
}

// SetWriteDeadline sets the deadline for future write_to calls
// and any currently-blocked write_to call.
// Even if write times out, it may return n > 0, indicating that
// some of the data was successfully written.
// A zero value for t means write_to will not time out.
func (c *UDPConn) SetWriteDeadline(t time.Time) error {
    // Write never blocks.
    return nil
}

func addr2PeerAddress(addr net.Addr) proto.PeerAddress {
    var peerAddr proto.PeerAddress
    switch a := addr.(type) {
    case *net.UDPAddr:
        peerAddr.IP = a.IP
        peerAddr.Port = a.Port
    case *net.TCPAddr:
        peerAddr.IP = a.IP
        peerAddr.Port = a.Port
    }

    return peerAddr
}

func (c *UDPConn) createPermissions(addrs ...net.Addr) error {
    setters := []stun.Setter{
        stun.TransactionID,
        stun.NewType(stun.MethodCreatePermission, stun.ClassRequest),
    }

    for _, addr := range addrs {
        setters = append(setters, addr2PeerAddress(addr))
    }

    setters = append(setters,
        c.obs.username(),
        c.obs.realm(),
        c.nonce(),
        c.integrity,
        stun.Fingerprint)

    msg, err := stun.Build(setters...)
    if err != nil {
        return err
    }

    trRes, err := c.obs.perform_transaction(msg, c.obs.turnserver_addr(), false)
    if err != nil {
        return err
    }

    res := trRes.Msg

    if res.Type.Class == stun.ClassErrorResponse {
        var code stun.ErrorCodeAttribute
        if err = code.GetFrom(res); err == nil {
            if code.Code == stun.CodeStaleNonce {
                c.setNonceFromMsg(res)
                return errTryAgain
            }
            return fmt.Errorf("%s (error %s)", res.Type, code) //nolint:goerr113
        }

        return fmt.Errorf("%s", res.Type) //nolint:goerr113
    }

    return nil
}

// HandleInbound passes inbound data in UDPConn
func (c *UDPConn) HandleInbound(data []byte, from net.Addr) {
    // copy data
    copied := make([]byte, len(data))
    copy(copied, data)

    select {
    case c.read_ch <- &InboundData{data: copied, from: from}:
    default:
        c.log.Warnf("receive buffer full")
    }
}

// FindAddrByChannelNumber returns a peer address associated with the
// channel number on this UDPConn
func (c *UDPConn) FindAddrByChannelNumber(chNum uint16) (net.Addr, bool) {
    b, ok := c.bindingMgr.findByNumber(chNum)
    if !ok {
        return nil, false
    }
    return b.addr, true
}

func (c *UDPConn) setNonceFromMsg(msg *stun.Message) {
    // Update nonce
    var nonce stun.nonce
    if err := nonce.GetFrom(msg); err == nil {
        c.setNonce(nonce)
        c.log.Debug("refresh allocation: 438, got new nonce.")
    } else {
        c.log.Warn("refresh allocation: 438 but no nonce.")
    }
}

func (c *UDPConn) refreshAllocation(lifetime time.Duration, dontWait bool) error {
    msg, err := stun.Build(
        stun.TransactionID,
        stun.NewType(stun.MethodRefresh, stun.ClassRequest),
        proto.lifetime{Duration: lifetime},
        c.obs.username(),
        c.obs.realm(),
        c.nonce(),
        c.integrity,
        stun.Fingerprint,
    )
    if err != nil {
        return fmt.Errorf("%w: %s", errFailedToBuildRefreshRequest, err.Error())
    }

    c.log.Debugf("send refresh request (dontWait=%v)", dontWait)
    trRes, err := c.obs.perform_transaction(msg, c.obs.turnserver_addr(), dontWait)
    if err != nil {
        return fmt.Errorf("%w: %s", errFailedToRefreshAllocation, err.Error())
    }

    if dontWait {
        c.log.Debug("refresh request sent")
        return nil
    }

    c.log.Debug("refresh request sent, and waiting response")

    res := trRes.Msg
    if res.Type.Class == stun.ClassErrorResponse {
        var code stun.ErrorCodeAttribute
        if err = code.GetFrom(res); err == nil {
            if code.Code == stun.CodeStaleNonce {
                c.setNonceFromMsg(res)
                return errTryAgain
            }
            return err
        }
        return fmt.Errorf("%s", res.Type) //nolint:goerr113
    }

    // Getting lifetime from response
    var updatedLifetime proto.lifetime
    if err := updatedLifetime.GetFrom(res); err != nil {
        return fmt.Errorf("%w: %s", errFailedToGetLifetime, err.Error())
    }

    c.setLifetime(updatedLifetime.Duration)
    c.log.Debugf("updated lifetime: %d seconds", int(c.lifetime().Seconds()))
    return nil
}

func (c *UDPConn) refreshPermissions() error {
    addrs := c.perm_map.addrs()
    if len(addrs) == 0 {
        c.log.Debug("no permission to refresh")
        return nil
    }
    if err := c.createPermissions(addrs...); err != nil {
        if errors.Is(err, errTryAgain) {
            return errTryAgain
        }
        c.log.Errorf("fail to refresh permissions: %s", err.Error())
        return err
    }
    c.log.Debug("refresh permissions successful")
    return nil
}

func (c *UDPConn) bind(b *binding) error {
    setters := []stun.Setter{
        stun.TransactionID,
        stun.NewType(stun.MethodChannelBind, stun.ClassRequest),
        addr2PeerAddress(b.addr),
        proto.ChannelNumber(b.number),
        c.obs.username(),
        c.obs.realm(),
        c.nonce(),
        c.integrity,
        stun.Fingerprint,
    }

    msg, err := stun.Build(setters...)
    if err != nil {
        return err
    }

    trRes, err := c.obs.perform_transaction(msg, c.obs.turnserver_addr(), false)
    if err != nil {
        c.bindingMgr.deleteByAddr(b.addr)
        return err
    }

    res := trRes.Msg

    if res.Type != stun.NewType(stun.MethodChannelBind, stun.ClassSuccessResponse) {
        return fmt.Errorf("unexpected response type %s", res.Type) //nolint:goerr113
    }

    c.log.Debugf("channel binding successful: %s %d", b.addr.String(), b.number)

    // Success.
    return nil
}

func (c *UDPConn) sendChannelData(data []byte, chNum uint16) (int, error) {
    chData := &proto.ChannelData{
        Data:   data,
        Number: proto.ChannelNumber(chNum),
    }
    chData.Encode()
    return c.obs.write_to(chData.Raw, c.obs.turnserver_addr())
}

func (c *UDPConn) onRefreshTimers(id int) {
    c.log.Debugf("refresh timer %d expired", id)
    switch id {
    case timerIDRefreshAlloc:
        var err error
        lifetime := c.lifetime()
        // limit the max retries on errTryAgain to 3
        // when stale nonce returns, sencond retry should succeed
        for i := 0; i < MAX_RETRY_ATTEMPTS; i++ {
            err = c.refreshAllocation(lifetime, false)
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
            err = c.refreshPermissions()
            if !errors.Is(err, errTryAgain) {
                break
            }
        }
        if err != nil {
            c.log.Warnf("refresh permissions failed")
        }
    }
}

func (c *UDPConn) nonce() stun.nonce {
    c.mutex.RLock()
    defer c.mutex.RUnlock()

    return c._nonce
}

func (c *UDPConn) setNonce(nonce stun.nonce) {
    c.mutex.Lock()
    defer c.mutex.Unlock()

    c.log.Debugf("set new nonce with %d bytes", len(nonce))
    c._nonce = nonce
}

func (c *UDPConn) lifetime() time.Duration {
    c.mutex.RLock()
    defer c.mutex.RUnlock()

    return c._lifetime
}

func (c *UDPConn) setLifetime(lifetime time.Duration) {
    c.mutex.Lock()
    defer c.mutex.Unlock()

    c._lifetime = lifetime
}
*/
