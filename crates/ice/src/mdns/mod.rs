// MulticastDNSMode represents the different Multicast modes ICE can run in
// MulticastDNSMode enum
#[derive(PartialEq, Debug, Copy, Clone)]
pub enum MulticastDNSMode {
    // MulticastDNSModeDisabled means remote mDNS candidates will be discarded, and local host candidates will use IPs
    Disabled,

    // MulticastDNSModeQueryOnly means remote mDNS candidates will be accepted, and local host candidates will use IPs
    QueryOnly,

    // MulticastDNSModeQueryAndGather means remote mDNS candidates will be accepted, and local host candidates will use mDNS
    QueryAndGather,
}

/*
func generateMulticastDNSName() (string, error) {
    // https://tools.ietf.org/id/draft-ietf-rtcweb-mdns-ice-candidates-02.html#gathering
    // The unique name MUST consist of a version 4 UUID as defined in [RFC4122], followed by “.local”.
    u, err := uuid.NewRandom()
    return u.String() + ".local", err
}

func createMulticastDNS(m_dnsmode MulticastDNSMode, mDNSName string, log logging.LeveledLogger) (*mdns.Conn, MulticastDNSMode, error) {
    if m_dnsmode == MulticastDNSModeDisabled {
        return nil, m_dnsmode, nil
    }

    addr, mdnsErr := net.ResolveUDPAddr("udp4", mdns.DefaultAddress)
    if mdnsErr != nil {
        return nil, m_dnsmode, mdnsErr
    }

    l, mdnsErr := net.ListenUDP("udp4", addr)
    if mdnsErr != nil {
        // If ICE fails to start MulticastDNS server just warn the user and continue
        log.Errorf("Failed to enable mDNS, continuing in mDNS disabled mode: (%s)", mdnsErr)
        return nil, MulticastDNSModeDisabled, nil
    }

    switch m_dnsmode {
    case MulticastDNSModeQueryOnly:
        conn, err := mdns.Server(ipv4.NewPacketConn(l), &mdns.Config{})
        return conn, m_dnsmode, err
    case MulticastDNSModeQueryAndGather:
        conn, err := mdns.Server(ipv4.NewPacketConn(l), &mdns.Config{
            LocalNames: []string{mDNSName},
        })
        return conn, m_dnsmode, err
    default:
        return nil, m_dnsmode, nil
    }
}
*/
