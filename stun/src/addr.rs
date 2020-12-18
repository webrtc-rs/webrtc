use std::net;

// MappedAddress represents MAPPED-ADDRESS attribute.
//
// This attribute is used only by servers for achieving backwards
// compatibility with RFC 3489 clients.
//
// RFC 5389 Section 15.1
pub struct MappedAddress {
    pub ip: net::IpAddr,
    pub port: u16,
}

// AlternateServer represents ALTERNATE-SERVER attribute.
//
// RFC 5389 Section 15.11
pub struct AlternateServer {
    pub ip: net::IpAddr,
    pub port: u16,
}

// ResponseOrigin represents RESPONSE-ORIGIN attribute.
//
// RFC 5780 Section 7.3
pub struct ResponseOrigin {
    pub ip: net::IpAddr,
    pub port: u16,
}

// OtherAddress represents OTHER-ADDRESS attribute.
//
// RFC 5780 Section 7.4
pub struct OtherAddress {
    pub ip: net::IpAddr,
    pub port: u16,
}
/*
impl AlternateServer {
    // add_to adds ALTERNATE-SERVER attribute to message.
    pub fn add_to(m *Message) error {
        a : = ( * MappedAddress)(s)
        return a.AddToAs(m, ATTR_ALTERNATE_SERVER)
    }

    // GetFrom decodes ALTERNATE-SERVER from message.
    func (s *AlternateServer) GetFrom(m *Message) error {
    a : = ( * MappedAddress)(s)
    return a.GetFromAs(m, ATTR_ALTERNATE_SERVER)
    }
}

func (a MappedAddress) String() string {
    return net.JoinHostPort(a.IP.String(), strconv.Itoa(a.Port))
}

// GetFromAs decodes MAPPED-ADDRESS value in message m as an attribute of type t.
func (a *MappedAddress) GetFromAs(m *Message, t AttrType) error {
    v, err := m.get(t)
    if err != nil {
        return err
    }
    if len(v) <= 4 {
        return io.ErrUnexpectedEOF
    }
    family := bin.Uint16(v[0:2])
    if family != familyIPv6 && family != familyIPv4 {
        return newDecodeErr("xor-mapped address", "family",
            fmt.Sprintf("bad value %d", family),
        )
    }
    ipLen := net.IPv4len
    if family == familyIPv6 {
        ipLen = net.IPv6len
    }
    // Ensuring len(a.IP) == ipLen and reusing a.IP.
    if len(a.IP) < ipLen {
        a.IP = a.IP[:cap(a.IP)]
        for len(a.IP) < ipLen {
            a.IP = append(a.IP, 0)
        }
    }
    a.IP = a.IP[:ipLen]
    for i := range a.IP {
        a.IP[i] = 0
    }
    a.Port = int(bin.Uint16(v[2:4]))
    copy(a.IP, v[4:])
    return nil
}

// AddToAs adds MAPPED-ADDRESS value to m as t attribute.
func (a *MappedAddress) AddToAs(m *Message, t AttrType) error {
    var (
        family = familyIPv4
        ip     = a.IP
    )
    if len(a.IP) == net.IPv6len {
        if isIPv4(ip) {
            ip = ip[12:16] // like in ip.To4()
        } else {
            family = familyIPv6
        }
    } else if len(ip) != net.IPv4len {
        return ErrBadIPLength
    }
    value := make([]byte, 128)
    value[0] = 0 // first 8 bits are zeroes
    bin.PutUint16(value[0:2], family)
    bin.PutUint16(value[2:4], uint16(a.Port))
    copy(value[4:], ip)
    m.Add(t, value[:4+len(ip)])
    return nil
}

// add_to adds MAPPED-ADDRESS to message.
func (a *MappedAddress) add_to(m *Message) error {
    return a.AddToAs(m, ATTR_MAPPED_ADDRESS)
}

// GetFrom decodes MAPPED-ADDRESS from message.
func (a *MappedAddress) GetFrom(m *Message) error {
    return a.GetFromAs(m, ATTR_MAPPED_ADDRESS)
}

// add_to adds OTHER-ADDRESS attribute to message.
func (o *OtherAddress) add_to(m *Message) error {
    a := (*MappedAddress)(o)
    return a.AddToAs(m, ATTR_OTHER_ADDRESS)
}

// GetFrom decodes OTHER-ADDRESS from message.
func (o *OtherAddress) GetFrom(m *Message) error {
    a := (*MappedAddress)(o)
    return a.GetFromAs(m, ATTR_OTHER_ADDRESS)
}

func (o OtherAddress) String() string {
    return net.JoinHostPort(o.IP.String(), strconv.Itoa(o.Port))
}

// add_to adds RESPONSE-ORIGIN attribute to message.
func (o *ResponseOrigin) add_to(m *Message) error {
    a := (*MappedAddress)(o)
    return a.AddToAs(m, ATTR_RESPONSE_ORIGIN)
}

// GetFrom decodes RESPONSE-ORIGIN from message.
func (o *ResponseOrigin) GetFrom(m *Message) error {
    a := (*MappedAddress)(o)
    return a.GetFromAs(m, ATTR_RESPONSE_ORIGIN)
}

func (o ResponseOrigin) String() string {
    return net.JoinHostPort(o.IP.String(), strconv.Itoa(o.Port))
}
*/
