#[cfg(test)]
mod net_test;

use super::conn_map::*;
use super::errors::*;
use crate::vnet::chunk::Chunk;
use crate::vnet::conn::ConnObserver;
use crate::vnet::router::*;
use crate::Error;

use async_trait::async_trait;
use ifaces::*;
use std::net::IpAddr;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

pub(crate) const LO0_STR: &str = "lo0";
pub(crate) const UDP_STR: &str = "udp";

lazy_static! {
    pub static ref MAC_ADDR_COUNTER: AtomicU64 = AtomicU64::new(0xBEEFED910200);
}

pub(crate) type HardwareAddr = Vec<u8>;

pub(crate) fn new_mac_address() -> HardwareAddr {
    let b = MAC_ADDR_COUNTER
        .fetch_add(1, Ordering::SeqCst)
        .to_be_bytes();
    b[2..].to_vec()
}

pub(crate) struct VNet {
    interfaces: Vec<Interface>,                // read-only
    static_ips: Vec<IpAddr>,                   // read-only
    router: Mutex<Option<Arc<Mutex<Router>>>>, // read-only
    udp_conns: UDPConnMap,                     // read-only
                                               //mutex      sync.RWMutex
}

#[async_trait]
impl NIC for VNet {
    fn get_interface(&self, ifc_name: &str) -> Option<&Interface> {
        for ifc in &self.interfaces {
            if ifc.name == ifc_name {
                return Some(ifc);
            }
        }
        None
    }

    async fn set_router(&self, r: Arc<Mutex<Router>>) -> Result<(), Error> {
        let mut router = self.router.lock().await;
        *router = Some(r);

        Ok(())
    }

    async fn on_inbound_chunk(&self, c: Box<dyn Chunk + Send + Sync>) {
        if c.network() == UDP_STR {
            if let Some(conn) = self.udp_conns.find(&c.destination_addr()).await {
                let tx = conn.get_inbound_ch();
                let _ = tx.send(c).await;
            }
        }
    }

    fn get_static_ips(&self) -> &[IpAddr] {
        &[]
    }
}

#[async_trait]
impl ConnObserver for VNet {
    async fn write(&self, c: Box<dyn Chunk + Send + Sync>) -> Result<(), Error> {
        if c.network() == UDP_STR && c.get_destination_ip().is_loopback() {
            if let Some(conn) = self.udp_conns.find(&c.destination_addr()).await {
                let tx = conn.get_inbound_ch();
                let _ = tx.send(c).await;
            }
            return Ok(());
        }

        let router = self.router.lock().await;
        if let Some(r) = &*router {
            let p = r.lock().await;
            p.push(c).await;
            Ok(())
        } else {
            Err(ERR_NO_ROUTER_LINKED.to_owned())
        }
    }

    // This method determines the srcIP based on the dstIP when locIP
    // is any IP address ("0.0.0.0" or "::"). If locIP is a non-any addr,
    // this method simply returns locIP.
    // caller must hold the mutex
    fn determine_source_ip(&self, loc_ip: IpAddr, dst_ip: IpAddr) -> Option<IpAddr> {
        if !loc_ip.is_unspecified() {
            return Some(loc_ip);
        }

        if dst_ip.is_loopback() {
            let src_ip = if let Ok(src_ip) = IpAddr::from_str("127.0.0.1") {
                Some(src_ip)
            } else {
                None
            };
            return src_ip;
        }

        if let Some(ifc) = self.get_interface("eth0") {
            if let Some(addr) = ifc.addr {
                //TODO: ipv4 vs ipv6?
                Some(addr.ip())
            } else {
                None
            }
        } else {
            None
        }
    }
}

impl VNet {
    pub(crate) fn get_interfaces(&self) -> &[Interface] {
        &self.interfaces
    }

    // caller must hold the mutex
    pub(crate) fn get_all_ip_addrs(&self, ipv6: bool) -> Vec<IpAddr> {
        let mut ips = vec![];

        for ifc in &self.interfaces {
            if let Some(addr) = ifc.addr {
                if (ipv6 && addr.is_ipv6()) || (!ipv6 && addr.is_ipv4()) {
                    ips.push(addr.ip());
                }
            }
        }

        ips
    }
}
/*


// caller must hold the mutex
func (v *vNet) _dialUDP(network string, locAddr, remAddr *net.UDPAddr) (UDPPacketConn, error) {
    // validate network
    if network != udpString && network != "udp4" {
        return nil, fmt.Errorf("%w: %s", errUnexpectedNetwork, network)
    }

    if locAddr == nil {
        locAddr = &net.UDPAddr{
            IP: net.IPv4zero,
        }
    } else if locAddr.IP == nil {
        locAddr.IP = net.IPv4zero
    }

    // validate address. do we have that address?
    if !v.hasIPAddr(locAddr.IP) {
        return nil, &net.OpError{
            Op:   "listen",
            Net:  network,
            Addr: locAddr,
            Err:  fmt.Errorf("bind: %w", errCantAssignRequestedAddr),
        }
    }

    if locAddr.Port == 0 {
        // choose randomly from the range between 5000 and 5999
        port, err := v.assignPort(locAddr.IP, 5000, 5999)
        if err != nil {
            return nil, &net.OpError{
                Op:   "listen",
                Net:  network,
                Addr: locAddr,
                Err:  err,
            }
        }
        locAddr.Port = port
    } else if _, ok := v.udp_conns.find(locAddr); ok {
        return nil, &net.OpError{
            Op:   "listen",
            Net:  network,
            Addr: locAddr,
            Err:  fmt.Errorf("bind: %w", errAddressAlreadyInUse),
        }
    }

    conn, err := newUDPConn(locAddr, remAddr, v)
    if err != nil {
        return nil, err
    }

    err = v.udp_conns.insert(conn)
    if err != nil {
        return nil, err
    }

    return conn, nil
}

func (v *vNet) listenPacket(network string, address string) (UDPPacketConn, error) {
    v.mutex.Lock()
    defer v.mutex.Unlock()

    locAddr, err := v.resolveUDPAddr(network, address)
    if err != nil {
        return nil, err
    }

    return v._dialUDP(network, locAddr, nil)
}

func (v *vNet) listenUDP(network string, locAddr *net.UDPAddr) (UDPPacketConn, error) {
    v.mutex.Lock()
    defer v.mutex.Unlock()

    return v._dialUDP(network, locAddr, nil)
}

func (v *vNet) dialUDP(network string, locAddr, remAddr *net.UDPAddr) (UDPPacketConn, error) {
    v.mutex.Lock()
    defer v.mutex.Unlock()

    return v._dialUDP(network, locAddr, remAddr)
}

func (v *vNet) dial(network string, address string) (UDPPacketConn, error) {
    v.mutex.Lock()
    defer v.mutex.Unlock()

    remAddr, err := v.resolveUDPAddr(network, address)
    if err != nil {
        return nil, err
    }

    // Determine source address
    srcIP := v.determineSourceIP(nil, remAddr.IP)

    locAddr := &net.UDPAddr{IP: srcIP, Port: 0}

    return v._dialUDP(network, locAddr, remAddr)
}

func (v *vNet) resolveUDPAddr(network, address string) (*net.UDPAddr, error) {
    if network != udpString && network != "udp4" {
        return nil, fmt.Errorf("%w %s", errUnknownNetwork, network)
    }

    host, sPort, err := net.SplitHostPort(address)
    if err != nil {
        return nil, err
    }

    // Check if host is a domain name
    ip := net.ParseIP(host)
    if ip == nil {
        host = strings.ToLower(host)
        if host == "localhost" {
            ip = net.IPv4(127, 0, 0, 1)
        } else {
            // host is a domain name. resolve IP address by the name
            if v.router == nil {
                return nil, errNoRouterLinked
            }

            ip, err = v.router.resolver.lookUp(host)
            if err != nil {
                return nil, err
            }
        }
    }

    port, err := strconv.Atoi(sPort)
    if err != nil {
        return nil, errInvalidPortNumber
    }

    udpAddr := &net.UDPAddr{
        IP:   ip,
        Port: port,
    }

    return udpAddr, nil
}


func (v *vNet) onClosed(addr net.Addr) {
    if addr.Network() == udpString {
        //nolint:errcheck
        v.udp_conns.delete(addr) // #nosec
    }
}

// caller must hold the mutex
func (v *vNet) hasIPAddr(ip net.IP) bool { //nolint:gocognit
    for _, ifc := range v.interfaces {
        if addrs, err := ifc.Addrs(); err == nil {
            for _, addr := range addrs {
                var locIP net.IP
                if ipNet, ok := addr.(*net.IPNet); ok {
                    locIP = ipNet.IP
                } else if ipAddr, ok := addr.(*net.IPAddr); ok {
                    locIP = ipAddr.IP
                } else {
                    continue
                }

                switch ip.String() {
                case "0.0.0.0":
                    if locIP.To4() != nil {
                        return true
                    }
                case "::":
                    if locIP.To4() == nil {
                        return true
                    }
                default:
                    if locIP.Equal(ip) {
                        return true
                    }
                }
            }
        }
    }

    return false
}

// caller must hold the mutex
func (v *vNet) allocateLocalAddr(ip net.IP, port int) error {
    // gather local IP addresses to bind
    var ips []net.IP
    if ip.IsUnspecified() {
        ips = v.getAllIPAddrs(ip.To4() == nil)
    } else if v.hasIPAddr(ip) {
        ips = []net.IP{ip}
    }

    if len(ips) == 0 {
        return fmt.Errorf("%w %s", errBindFailerFor, ip.String())
    }

    // check if all these transport addresses are not in use
    for _, ip2 := range ips {
        addr := &net.UDPAddr{
            IP:   ip2,
            Port: port,
        }
        if _, ok := v.udp_conns.find(addr); ok {
            return &net.OpError{
                Op:   "bind",
                Net:  udpString,
                Addr: addr,
                Err:  fmt.Errorf("bind: %w", errAddressAlreadyInUse),
            }
        }
    }

    return nil
}

// caller must hold the mutex
func (v *vNet) assignPort(ip net.IP, start, end int) (int, error) {
    // choose randomly from the range between start and end (inclusive)
    if end < start {
        return -1, errEndPortLessThanStart
    }

    space := end + 1 - start
    offset := rand.Intn(space) //nolint:gosec
    for i := 0; i < space; i++ {
        port := ((offset + i) % space) + start

        err := v.allocateLocalAddr(ip, port)
        if err == nil {
            return port, nil
        }
    }

    return -1, errPortSpaceExhausted
}

// NetConfig is a bag of configuration parameters passed to NewNet().
type NetConfig struct {
    // StaticIPs is an array of static IP addresses to be assigned for this Net.
    // If no static IP address is given, the router will automatically assign
    // an IP address.
    StaticIPs []string

    // StaticIP is deprecated. Use StaticIPs.
    StaticIP string
}

// Net represents a local network stack euivalent to a set of layers from NIC
// up to the transport (UDP / TCP) layer.
type Net struct {
    v   *vNet
    ifs []*Interface
}

// NewNet creates an instance of Net.
// If config is nil, the virtual network is disabled. (uses corresponding
// net.Xxxx() operations.
// By design, it always have lo0 and eth0 interfaces.
// The lo0 has the address 127.0.0.1 assigned by default.
// IP address for eth0 will be assigned when this Net is added to a router.
func NewNet(config *NetConfig) *Net {
    if config == nil {
        ifs := []*Interface{}
        if orgIfs, err := net.Interfaces(); err == nil {
            for _, orgIfc := range orgIfs {
                ifc := NewInterface(orgIfc)
                if addrs, err := orgIfc.Addrs(); err == nil {
                    for _, addr := range addrs {
                        ifc.AddAddr(addr)
                    }
                }

                ifs = append(ifs, ifc)
            }
        }

        return &Net{ifs: ifs}
    }

    lo0 := NewInterface(net.Interface{
        Index:        1,
        MTU:          16384,
        Name:         lo0String,
        HardwareAddr: nil,
        Flags:        net.FlagUp | net.FlagLoopback | net.FlagMulticast,
    })
    lo0.AddAddr(&net.IPNet{
        IP:   net.ParseIP("127.0.0.1"),
        Mask: net.CIDRMask(8, 32),
    })

    eth0 := NewInterface(net.Interface{
        Index:        2,
        MTU:          1500,
        Name:         "eth0",
        HardwareAddr: newMACAddress(),
        Flags:        net.FlagUp | net.FlagMulticast,
    })

    var static_ips []net.IP
    for _, ipStr := range config.StaticIPs {
        if ip := net.ParseIP(ipStr); ip != nil {
            static_ips = append(static_ips, ip)
        }
    }
    if len(config.StaticIP) > 0 {
        if ip := net.ParseIP(config.StaticIP); ip != nil {
            static_ips = append(static_ips, ip)
        }
    }

    v := &vNet{
        interfaces: []*Interface{lo0, eth0},
        static_ips:  static_ips,
        udp_conns:   newUDPConnMap(),
    }

    return &Net{
        v: v,
    }
}

// Interfaces returns a list of the system's network interfaces.
func (n *Net) Interfaces() ([]*Interface, error) {
    if n.v == nil {
        return n.ifs, nil
    }

    return n.v.getInterfaces()
}

// InterfaceByName returns the interface specified by name.
func (n *Net) InterfaceByName(name string) (*Interface, error) {
    if n.v == nil {
        for _, ifc := range n.ifs {
            if ifc.Name == name {
                return ifc, nil
            }
        }

        return nil, fmt.Errorf("interface %s %w", name, errNotFound)
    }

    return n.v.get_interface(name)
}

// ListenPacket announces on the local network address.
func (n *Net) ListenPacket(network string, address string) (net.PacketConn, error) {
    if n.v == nil {
        return net.ListenPacket(network, address)
    }

    return n.v.listenPacket(network, address)
}

// ListenUDP acts like ListenPacket for UDP networks.
func (n *Net) ListenUDP(network string, locAddr *net.UDPAddr) (UDPPacketConn, error) {
    if n.v == nil {
        return net.ListenUDP(network, locAddr)
    }

    return n.v.listenUDP(network, locAddr)
}

// Dial connects to the address on the named network.
func (n *Net) Dial(network, address string) (net.Conn, error) {
    if n.v == nil {
        return net.Dial(network, address)
    }

    return n.v.dial(network, address)
}

// CreateDialer creates an instance of vnet.Dialer
func (n *Net) CreateDialer(dialer *net.Dialer) Dialer {
    if n.v == nil {
        return &vDialer{
            dialer: dialer,
        }
    }

    return &vDialer{
        dialer: dialer,
        v:      n.v,
    }
}

// DialUDP acts like Dial for UDP networks.
func (n *Net) DialUDP(network string, laddr, raddr *net.UDPAddr) (UDPPacketConn, error) {
    if n.v == nil {
        return net.DialUDP(network, laddr, raddr)
    }

    return n.v.dialUDP(network, laddr, raddr)
}

// ResolveUDPAddr returns an address of UDP end point.
func (n *Net) ResolveUDPAddr(network, address string) (*net.UDPAddr, error) {
    if n.v == nil {
        return net.ResolveUDPAddr(network, address)
    }

    return n.v.resolveUDPAddr(network, address)
}

func (n *Net) get_interface(ifName string) (*Interface, error) {
    if n.v == nil {
        return nil, errVNetDisabled
    }

    return n.v.get_interface(ifName)
}

func (n *Net) set_router(r *Router) error {
    if n.v == nil {
        return errVNetDisabled
    }

    return n.v.set_router(r)
}

func (n *Net) on_inbound_chunk(c Chunk) {
    if n.v == nil {
        return
    }

    n.v.on_inbound_chunk(c)
}

func (n *Net) get_static_ips() []net.IP {
    if n.v == nil {
        return nil
    }

    return n.v.static_ips
}

// IsVirtual tests if the virtual network is enabled.
func (n *Net) IsVirtual() bool {
    return n.v != nil
}

// Dialer is identical to net.Dialer excepts that its methods
// (Dial, DialContext) are overridden to use virtual network.
// Use vnet.CreateDialer() to create an instance of this Dialer.
type Dialer interface {
    Dial(network, address string) (net.Conn, error)
}

type vDialer struct {
    dialer *net.Dialer
    v      *vNet
}

func (d *vDialer) Dial(network, address string) (net.Conn, error) {
    if d.v == nil {
        return d.dialer.Dial(network, address)
    }

    return d.v.dial(network, address)
}
*/
