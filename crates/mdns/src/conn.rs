use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use tokio::sync::mpsc;

const INBOUND_BUFFER_SIZE: usize = 512;
const DEFAULT_QUERY_INTERVAL: Duration = Duration::from_secs(1);
const DESTINATION_ADDRESS: &str = "224.0.0.251:5353";
const MAX_MESSAGE_RECORDS: usize = 3;
const RESPONSE_TTL: usize = 120;

// Conn represents a mDNS Server
pub struct Conn {
    //mu  sync.RWMutex
    //log logging.LeveledLogger
    socket: SocketAddr, //*ipv4.PacketConn
    dst_addr: IpAddr,   //*net.UDPAddr

    query_interval: Duration,
    local_names: Vec<String>,
    queries: Vec<Query>,

    closed: mpsc::Receiver<()>, //chan interface{}
}

struct Query {
    name_with_suffix: String,
    query_result_chan: mpsc::Sender<QueryResult>,
}

struct QueryResult {
    //answer dnsmessage.ResourceHeader
//addr   :net.Addr
}

/*
// Server establishes a mDNS connection over an existing conn
func Server(conn *ipv4.PacketConn, config *Config) (*Conn, error) {
    if config == nil {
        return nil, errNilConfig
    }

    ifaces, err := net.Interfaces()
    if err != nil {
        return nil, err
    }

    joinErrCount := 0
    for i := range ifaces {
        if err = conn.JoinGroup(&ifaces[i], &net.UDPAddr{IP: net.IPv4(224, 0, 0, 251)}); err != nil {
            joinErrCount++
        }
    }
    if joinErrCount >= len(ifaces) {
        return nil, errJoiningMulticastGroup
    }

    dst_addr, err := net.ResolveUDPAddr("udp", DESTINATION_ADDRESS)
    if err != nil {
        return nil, err
    }

    loggerFactory := config.LoggerFactory
    if loggerFactory == nil {
        loggerFactory = logging.NewDefaultLoggerFactory()
    }

    local_names := []string{}
    for _, l := range config.LocalNames {
        local_names = append(local_names, l+".")
    }

    c := &Conn{
        query_interval: DEFAULT_QUERY_INTERVAL,
        queries:       []Query{},
        socket:        conn,
        dst_addr:       dst_addr,
        local_names:    local_names,
        log:           loggerFactory.NewLogger("mdns"),
        closed:        make(chan interface{}),
    }
    if config.QueryInterval != 0 {
        c.query_interval = config.QueryInterval
    }

    go c.start()
    return c, nil
}

// Close closes the mDNS Conn
func (c *Conn) Close() error {
    select {
    case <-c.closed:
        return nil
    default:
    }

    if err := c.socket.Close(); err != nil {
        return err
    }

    <-c.closed
    return nil
}

// Query sends mDNS Queries for the following name until
// either the Context is canceled/expires or we get a result
func (c *Conn) Query(ctx context.Context, name string) (dnsmessage.ResourceHeader, net.Addr, error) {
    select {
    case <-c.closed:
        return dnsmessage.ResourceHeader{}, nil, errConnectionClosed
    default:
    }

    name_with_suffix := name + "."

    queryChan := make(chan QueryResult, 1)
    c.mu.Lock()
    c.queries = append(c.queries, Query{name_with_suffix, queryChan})
    ticker := time.NewTicker(c.query_interval)
    c.mu.Unlock()

    c.sendQuestion(name_with_suffix)
    for {
        select {
        case <-ticker.C:
            c.sendQuestion(name_with_suffix)
        case <-c.closed:
            return dnsmessage.ResourceHeader{}, nil, errConnectionClosed
        case res := <-queryChan:
            return res.answer, res.addr, nil
        case <-ctx.Done():
            return dnsmessage.ResourceHeader{}, nil, errContextElapsed
        }
    }
}

func ipToBytes(ip net.IP) (out [4]byte) {
    rawIP := ip.To4()
    if rawIP == nil {
        return
    }

    ipInt := big.NewInt(0)
    ipInt.SetBytes(rawIP)
    copy(out[:], ipInt.Bytes())
    return
}

func interfaceForRemote(remote string) (net.IP, error) {
    conn, err := net.Dial("udp", remote)
    if err != nil {
        return nil, err
    }

    localAddr := conn.LocalAddr().(*net.UDPAddr)
    if err := conn.Close(); err != nil {
        return nil, err
    }

    return localAddr.IP, nil
}

func (c *Conn) sendQuestion(name string) {
    packedName, err := dnsmessage.NewName(name)
    if err != nil {
        c.log.Warnf("Failed to construct mDNS packet %v", err)
        return
    }

    msg := dnsmessage.Message{
        Header: dnsmessage.Header{},
        Questions: []dnsmessage.question{
            {
                Type:  dnsmessage.TypeA,
                Class: dnsmessage.ClassINET,
                Name:  packedName,
            },
        },
    }

    rawQuery, err := msg.Pack()
    if err != nil {
        c.log.Warnf("Failed to construct mDNS packet %v", err)
        return
    }

    if _, err := c.socket.WriteTo(rawQuery, nil, c.dst_addr); err != nil {
        c.log.Warnf("Failed to send mDNS packet %v", err)
        return
    }
}

func (c *Conn) sendAnswer(name string, dst net.IP) {
    packedName, err := dnsmessage.NewName(name)
    if err != nil {
        c.log.Warnf("Failed to construct mDNS packet %v", err)
        return
    }

    msg := dnsmessage.Message{
        Header: dnsmessage.Header{
            Response:      true,
            Authoritative: true,
        },
        Answers: []dnsmessage.Resource{
            {
                Header: dnsmessage.ResourceHeader{
                    Type:  dnsmessage.TypeA,
                    Class: dnsmessage.ClassINET,
                    Name:  packedName,
                    TTL:   RESPONSE_TTL,
                },
                Body: &dnsmessage.AResource{
                    A: ipToBytes(dst),
                },
            },
        },
    }

    rawAnswer, err := msg.Pack()
    if err != nil {
        c.log.Warnf("Failed to construct mDNS packet %v", err)
        return
    }

    if _, err := c.socket.WriteTo(rawAnswer, nil, c.dst_addr); err != nil {
        c.log.Warnf("Failed to send mDNS packet %v", err)
        return
    }
}

func (c *Conn) start() { //nolint gocognit
    defer func() {
        c.mu.Lock()
        defer c.mu.Unlock()
        close(c.closed)
    }()

    b := make([]byte, INBOUND_BUFFER_SIZE)
    p := dnsmessage.Parser{}

    for {
        n, _, src, err := c.socket.ReadFrom(b)
        if err != nil {
            return
        }

        func() {
            c.mu.RLock()
            defer c.mu.RUnlock()

            if _, err := p.start(b[:n]); err != nil {
                c.log.Warnf("Failed to parse mDNS packet %v", err)
                return
            }

            for i := 0; i <= MAX_MESSAGE_RECORDS; i++ {
                q, err := p.question()
                if errors.Is(err, dnsmessage.ErrSectionDone) {
                    break
                } else if err != nil {
                    c.log.Warnf("Failed to parse mDNS packet %v", err)
                    return
                }

                for _, localName := range c.local_names {
                    if localName == q.Name.String() {
                        localAddress, err := interfaceForRemote(src.String())
                        if err != nil {
                            c.log.Warnf("Failed to get local interface to communicate with %s: %v", src.String(), err)
                            continue
                        }

                        c.sendAnswer(q.Name.String(), localAddress)
                    }
                }
            }

            for i := 0; i <= MAX_MESSAGE_RECORDS; i++ {
                a, err := p.answer_header()
                if errors.Is(err, dnsmessage.ErrSectionDone) {
                    return
                }
                if err != nil {
                    c.log.Warnf("Failed to parse mDNS packet %v", err)
                    return
                }

                if a.Type != dnsmessage.TypeA && a.Type != dnsmessage.TypeAAAA {
                    continue
                }

                for i := len(c.queries) - 1; i >= 0; i-- {
                    if c.queries[i].name_with_suffix == a.Name.String() {
                        c.queries[i].query_result_chan <- QueryResult{a, src}
                        c.queries = append(c.queries[:i], c.queries[i+1:]...)
                    }
                }
            }
        }()
    }
}*/
