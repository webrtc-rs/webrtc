use crate::vnet::chunk::Chunk;
use crate::vnet::chunk_queue::ChunkQueue;
use crate::vnet::interface::Interface;
use crate::vnet::nat::*;
use crate::Error;

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::mpsc;
use tokio::time::Duration;

use crate::vnet::resolver::Resolver;
use std::sync::Arc;

const DEFAULT_ROUTER_QUEUE_SIZE: usize = 0; // unlimited

lazy_static! {
    pub static ref ROUTER_ID_CTR: AtomicU64 = AtomicU64::new(0);
}

// Generate a unique router name
fn assign_router_name() -> String {
    let n = ROUTER_ID_CTR.fetch_add(1, Ordering::SeqCst);
    format!("router{}", n)
}

// RouterConfig ...
#[derive(Default)]
pub struct RouterConfig {
    // name of router. If not specified, a unique name will be assigned.
    name: String,
    // cidr notation, like "192.0.2.0/24"
    cidr: String,
    // static_ips is an array of static IP addresses to be assigned for this router.
    // If no static IP address is given, the router will automatically assign
    // an IP address.
    // This will be ignored if this router is the root.
    static_ips: Vec<String>,
    // static_ip is deprecated. Use static_ips.
    static_ip: String,
    // Internal queue size
    queue_size: usize,
    // Effective only when this router has a parent router
    nat_type: NATType,
    // Minimum Delay
    min_delay: Duration,
    // Max Jitter
    max_jitter: Duration,
}

// NIC is a nework inerface controller that interfaces Router
pub trait NIC {
    fn get_interface(&self, if_name: &str) -> Result<Interface, Error>;
    fn on_inbound_chunk(&self, c: &dyn Chunk);
    fn get_static_ips(&self) -> Vec<IpAddr>;
    fn set_router(&mut self, r: Router) -> Result<(), Error>;
}

// ChunkFilter is a handler users can add to filter chunks.
// If the filter returns false, the packet will be dropped.
pub type ChunkFilterFn = fn(c: &dyn Chunk) -> bool;

// Router ...
pub struct Router {
    name: String,               // read-only
    interfaces: Vec<Interface>, // read-only
    //TODO: ipv4Net        :*net.IPNet                // read-only
    static_ips: Vec<IpAddr>,                   // read-only
    static_local_ips: HashMap<String, IpAddr>, // read-only,
    last_id: u8, // requires mutex [x], used to assign the last digit of IPv4 address
    queue: ChunkQueue, // read-only
    parent: Option<Arc<Router>>, // read-only
    children: Vec<Arc<Router>>, // read-only
    nat_type: NATType, // read-only
    nat: NetworkAddressTranslator, // read-only
    nics: HashMap<String, Box<dyn NIC>>, // read-only
    //TODO: stopFunc       :func()                    // requires mutex [x]
    resolver: Resolver,                // read-only
    chunk_filters: Vec<ChunkFilterFn>, // requires mutex [x]
    min_delay: Duration,               // requires mutex [x]
    max_jitter: Duration,              // requires mutex [x]
    //mutex          :sync.RWMutex              // thread-safe
    push_ch: mpsc::Sender<()>, // writer requires mutex
}
/*
// NewRouter ...
func NewRouter(config *RouterConfig) (*Router, error) {
    loggerFactory := config.LoggerFactory
    log := loggerFactory.NewLogger("vnet")

    _, ipv4Net, err := net.ParseCIDR(config.cidr)
    if err != nil {
        return nil, err
    }

    queueSize := defaultRouterQueueSize
    if config.queue_size > 0 {
        queueSize = config.queue_size
    }

    // set up network interface, lo0
    lo0 := NewInterface(net.Interface{
        Index:        1,
        MTU:          16384,
        name:         lo0String,
        HardwareAddr: nil,
        Flags:        net.FlagUp | net.FlagLoopback | net.FlagMulticast,
    })
    lo0.AddAddr(&net.IPAddr{IP: net.ParseIP("127.0.0.1"), Zone: ""})

    // set up network interface, eth0
    eth0 := NewInterface(net.Interface{
        Index:        2,
        MTU:          1500,
        name:         "eth0",
        HardwareAddr: newMACAddress(),
        Flags:        net.FlagUp | net.FlagMulticast,
    })

    // local host name resolver
    resolver := newResolver(&resolverConfig{
        LoggerFactory: config.LoggerFactory,
    })

    name := config.name
    if len(name) == 0 {
        name = assignRouterName()
    }

    var static_ips []net.IP
    static_local_ips := map[string]net.IP{}
    for _, ipStr := range config.static_ips {
        ipPair := strings.Split(ipStr, "/")
        if ip := net.ParseIP(ipPair[0]); ip != nil {
            if len(ipPair) > 1 {
                locIP := net.ParseIP(ipPair[1])
                if locIP == nil {
                    return nil, errInvalidLocalIPinStaticIPs
                }
                if !ipv4Net.Contains(locIP) {
                    return nil, fmt.Errorf("local IP %s %w", locIP.String(), errLocalIPBeyondStaticIPsSubset)
                }
                static_local_ips[ip.String()] = locIP
            }
            static_ips = append(static_ips, ip)
        }
    }
    if len(config.static_ip) > 0 {
        log.Warn("static_ip is deprecated. Use static_ips instead")
        if ip := net.ParseIP(config.static_ip); ip != nil {
            static_ips = append(static_ips, ip)
        }
    }

    if nStaticLocal := len(static_local_ips); nStaticLocal > 0 {
        if nStaticLocal != len(static_ips) {
            return nil, errLocalIPNoStaticsIPsAssociated
        }
    }

    return &Router{
        name:           name,
        interfaces:     []*Interface{lo0, eth0},
        ipv4Net:        ipv4Net,
        static_ips:      static_ips,
        static_local_ips: static_local_ips,
        queue:          newChunkQueue(queueSize),
        nat_type:        config.nattype,
        nics:           map[string]NIC{},
        resolver:       resolver,
        min_delay:       config.min_delay,
        max_jitter:      config.max_jitter,
        push_ch:         make(chan struct{}, 1),
        loggerFactory:  loggerFactory,
        log:            log,
    }, nil
}

// caller must hold the mutex
func (r *Router) getInterfaces() ([]*Interface, error) {
    if len(r.interfaces) == 0 {
        return nil, fmt.Errorf("%w is available", errNoInterface)
    }

    return r.interfaces, nil
}

func (r *Router) get_interface(ifName string) (*Interface, error) {
    r.mutex.RLock()
    defer r.mutex.RUnlock()

    ifs, err := r.getInterfaces()
    if err != nil {
        return nil, err
    }
    for _, ifc := range ifs {
        if ifc.name == ifName {
            return ifc, nil
        }
    }

    return nil, fmt.Errorf("interface %s %w", ifName, errNotFound)
}

// Start ...
func (r *Router) Start() error {
    r.mutex.Lock()
    defer r.mutex.Unlock()

    if r.stopFunc != nil {
        return errRouterAlreadyStarted
    }

    cancelCh := make(chan struct{})

    go func() {
    loop:
        for {
            d, err := r.processChunks()
            if err != nil {
                r.log.Errorf("[%s] %s", r.name, err.Error())
                break
            }

            if d <= 0 {
                select {
                case <-r.push_ch:
                case <-cancelCh:
                    break loop
                }
            } else {
                t := time.NewTimer(d)
                select {
                case <-t.C:
                case <-cancelCh:
                    break loop
                }
            }
        }
    }()

    r.stopFunc = func() {
        close(cancelCh)
    }

    for _, child := range r.children {
        if err := child.Start(); err != nil {
            return err
        }
    }

    return nil
}

// Stop ...
func (r *Router) Stop() error {
    r.mutex.Lock()
    defer r.mutex.Unlock()

    if r.stopFunc == nil {
        return errRouterAlreadyStopped
    }

    for _, router := range r.children {
        r.mutex.Unlock()
        err := router.Stop()
        r.mutex.Lock()

        if err != nil {
            return err
        }
    }

    r.stopFunc()
    r.stopFunc = nil
    return nil
}

// caller must hold the mutex
func (r *Router) addNIC(nic NIC) error {
    ifc, err := nic.get_interface("eth0")
    if err != nil {
        return err
    }

    var ips []net.IP

    if ips = nic.get_static_ips(); len(ips) == 0 {
        // assign an IP address
        ip, err2 := r.assignIPAddress()
        if err2 != nil {
            return err2
        }
        ips = append(ips, ip)
    }

    for _, ip := range ips {
        if !r.ipv4Net.Contains(ip) {
            return fmt.Errorf("%w: %s", errStaticIPisBeyondSubnet, r.ipv4Net.String())
        }

        ifc.AddAddr(&net.IPNet{
            IP:   ip,
            Mask: r.ipv4Net.Mask,
        })

        r.nics[ip.String()] = nic
    }

    if err = nic.set_router(r); err != nil {
        return err
    }

    return nil
}

// AddRouter adds a chile Router.
func (r *Router) AddRouter(router *Router) error {
    r.mutex.Lock()
    defer r.mutex.Unlock()

    // Router is a NIC. Add it as a NIC so that packets are routed to this child
    // router.
    err := r.addNIC(router)
    if err != nil {
        return err
    }

    if err = router.set_router(r); err != nil {
        return err
    }

    r.children = append(r.children, router)
    return nil
}

// AddNet ...
func (r *Router) AddNet(nic NIC) error {
    r.mutex.Lock()
    defer r.mutex.Unlock()

    return r.addNIC(nic)
}

// AddHost adds a mapping of hostname and an IP address to the local resolver.
func (r *Router) AddHost(hostName string, ipAddr string) error {
    return r.resolver.addHost(hostName, ipAddr)
}

// AddChunkFilter adds a filter for chunks traversing this router.
// You may add more than one filter. The filters are called in the order of this method call.
// If a chunk is dropped by a filter, subsequent filter will not receive the chunk.
func (r *Router) AddChunkFilter(filter ChunkFilter) {
    r.mutex.Lock()
    defer r.mutex.Unlock()

    r.chunk_filters = append(r.chunk_filters, filter)
}

// caller should hold the mutex
func (r *Router) assignIPAddress() (net.IP, error) {
    // See: https://stackoverflow.com/questions/14915188/ip-address-ending-with-zero

    if r.last_id == 0xfe {
        return nil, errAddressSpaceExhausted
    }

    ip := make(net.IP, 4)
    copy(ip, r.ipv4Net.IP[:3])
    r.last_id++
    ip[3] = r.last_id
    return ip, nil
}

func (r *Router) push(c Chunk) {
    r.mutex.Lock()
    defer r.mutex.Unlock()

    r.log.Debugf("[%s] route %s", r.name, c.String())
    if r.stopFunc != nil {
        c.setTimestamp()
        if r.queue.push(c) {
            select {
            case r.push_ch <- struct{}{}:
            default:
            }
        } else {
            r.log.Warnf("[%s] queue was full. dropped a chunk", r.name)
        }
    }
}

func (r *Router) processChunks() (time.Duration, error) {
    r.mutex.Lock()
    defer r.mutex.Unlock()

    // Introduce jitter by delaying the processing of chunks.
    if r.max_jitter > 0 {
        jitter := time.Duration(rand.Int63n(int64(r.max_jitter))) //nolint:gosec
        time.Sleep(jitter)
    }

    //      cutOff
    //         v min delay
    //         |<--->|
    //  +------------:--
    //  |OOOOOOXXXXX :   --> time
    //  +------------:--
    //  |<--->|     now
    //    due

    enteredAt := time.Now()
    cutOff := enteredAt.Add(-r.min_delay)

    var d time.Duration // the next sleep duration

    for {
        d = 0

        c := r.queue.peek()
        if c == nil {
            break // no more chunk in the queue
        }

        // check timestamp to find if the chunk is due
        if c.getTimestamp().After(cutOff) {
            // There is one or more chunk in the queue but none of them are due.
            // Calculate the next sleep duration here.
            nextExpire := c.getTimestamp().Add(r.min_delay)
            d = nextExpire.Sub(enteredAt)
            break
        }

        var ok bool
        if c, ok = r.queue.pop(); !ok {
            break // no more chunk in the queue
        }

        blocked := false
        for i := 0; i < len(r.chunk_filters); i++ {
            filter := r.chunk_filters[i]
            if !filter(c) {
                blocked = true
                break
            }
        }
        if blocked {
            continue // discard
        }

        dstIP := c.getDestinationIP()

        // check if the desination is in our subnet
        if r.ipv4Net.Contains(dstIP) {
            // search for the destination NIC
            var nic NIC
            if nic, ok = r.nics[dstIP.String()]; !ok {
                // NIC not found. drop it.
                r.log.Debugf("[%s] %s unreachable", r.name, c.String())
                continue
            }

            // found the NIC, forward the chunk to the NIC.
            // call to NIC must unlock mutex
            r.mutex.Unlock()
            nic.on_inbound_chunk(c)
            r.mutex.Lock()
            continue
        }

        // the destination is outside of this subnet
        // is this WAN?
        if r.parent == nil {
            // this WAN. No route for this chunk
            r.log.Debugf("[%s] no route found for %s", r.name, c.String())
            continue
        }

        // Pass it to the parent via NAT
        toParent, err := r.nat.translateOutbound(c)
        if err != nil {
            return 0, err
        }

        if toParent == nil {
            continue
        }

        //nolint:godox
        /* FIXME: this implementation would introduce a duplicate packet!
        if r.nat.nat_type.Hairpining {
            hairpinned, err := r.nat.translateInbound(toParent)
            if err != nil {
                r.log.Warnf("[%s] %s", r.name, err.Error())
            } else {
                go func() {
                    r.push(hairpinned)
                }()
            }
        }
        */

        // call to parent router mutex unlock mutex
        r.mutex.Unlock()
        r.parent.push(toParent)
        r.mutex.Lock()
    }

    return d, nil
}

// caller must hold the mutex
func (r *Router) set_router(parent *Router) error {
    r.parent = parent
    r.resolver.setParent(parent.resolver)

    // when this method is called, one or more IP address has already been assigned by
    // the parent router.
    ifc, err := r.get_interface("eth0")
    if err != nil {
        return err
    }

    if len(ifc.addrs) == 0 {
        return errNoIPAddrEth0
    }

    mappedIPs := []net.IP{}
    localIPs := []net.IP{}

    for _, ifcAddr := range ifc.addrs {
        var ip net.IP
        switch addr := ifcAddr.(type) {
        case *net.IPNet:
            ip = addr.IP
        case *net.IPAddr: // Do we really need this case?
            ip = addr.IP
        default:
        }

        if ip == nil {
            continue
        }

        mappedIPs = append(mappedIPs, ip)

        if locIP := r.static_local_ips[ip.String()]; locIP != nil {
            localIPs = append(localIPs, locIP)
        }
    }

    // Set up NAT here
    if r.nat_type == nil {
        r.nat_type = &nattype{
            MappingBehavior:   EndpointIndependent,
            FilteringBehavior: EndpointAddrPortDependent,
            Hairpining:        false,
            PortPreservation:  false,
            MappingLifeTime:   30 * time.Second,
        }
    }
    r.nat, err = newNAT(&natConfig{
        name:          r.name,
        nat_type:       *r.nat_type,
        mappedIPs:     mappedIPs,
        localIPs:      localIPs,
        loggerFactory: r.loggerFactory,
    })
    if err != nil {
        return err
    }

    return nil
}

func (r *Router) on_inbound_chunk(c Chunk) {
    fromParent, err := r.nat.translateInbound(c)
    if err != nil {
        r.log.Warnf("[%s] %s", r.name, err.Error())
        return
    }

    r.push(fromParent)
}

func (r *Router) get_static_ips() []net.IP {
    return r.static_ips
}
 */
