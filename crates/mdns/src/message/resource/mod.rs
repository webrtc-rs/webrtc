pub mod cname;

use super::name::*;
use super::*;

use std::collections::HashMap;
use std::fmt;

use util::Error;

// A Resource is a DNS resource record.
pub struct Resource {
    header: ResourceHeader,
    body: Box<dyn ResourceBody>,
}

/*
func (r *Resource) GoString() string {
    return "dnsmessage.Resource{" +
        "Header: " + r.Header.GoString() +
        ", Body: &" + r.Body.GoString() +
        "}"
}


func skipResource(msg []byte, off int) (int, error) {
    newOff, err := skip_name(msg, off)
    if err != nil {
        return off, &nestedError{"Name", err}
    }
    if newOff, err = skipType(msg, newOff); err != nil {
        return off, &nestedError{"Type", err}
    }
    if newOff, err = skipClass(msg, newOff); err != nil {
        return off, &nestedError{"Class", err}
    }
    if newOff, err = skipUint32(msg, newOff); err != nil {
        return off, &nestedError{"TTL", err}
    }
    length, newOff, err := unpackUint16(msg, newOff)
    if err != nil {
        return off, &nestedError{"Length", err}
    }
    if newOff += int(length); newOff > len(msg) {
        return off, errResourceLen
    }
    return newOff, nil
}

 */

// A ResourceHeader is the header of a DNS resource record. There are
// many types of DNS resource records, but they all share the same header.
pub struct ResourceHeader {
    // Name is the domain name for which this resource record pertains.
    name: Name,

    // Type is the type of DNS resource record.
    //
    // This field will be set automatically during packing.
    typ: DNSType,

    // Class is the class of network to which this DNS resource record
    // pertains.
    class: DNSClass,

    // TTL is the length of time (measured in seconds) which this resource
    // record is valid for (time to live). All Resources in a set should
    // have the same TTL (RFC 2181 Section 5.2).
    ttl: u32,

    // Length is the length of data in the resource record after the header.
    //
    // This field will be set automatically during packing.
    length: u16,
}

/*
// GoString implements fmt.GoStringer.GoString.
func (h *ResourceHeader) GoString() string {
    return "dnsmessage.ResourceHeader{" +
        "Name: " + h.Name.GoString() + ", " +
        "Type: " + h.Type.GoString() + ", " +
        "Class: " + h.Class.GoString() + ", " +
        "TTL: " + printUint32(h.TTL) + ", " +
        "Length: " + printUint16(h.Length) + "}"
}

// pack appends the wire format of the ResourceHeader to oldMsg.
//
// lenOff is the offset in msg where the Length field was packed.
func (h *ResourceHeader) pack(oldMsg []byte, compression map[string]int, compressionOff int) (msg []byte, lenOff int, err error) {
    msg = oldMsg
    if msg, err = h.Name.pack(msg, compression, compressionOff); err != nil {
        return oldMsg, 0, &nestedError{"Name", err}
    }
    msg = packType(msg, h.Type)
    msg = packClass(msg, h.Class)
    msg = packUint32(msg, h.TTL)
    lenOff = len(msg)
    msg = packUint16(msg, h.Length)
    return msg, lenOff, nil
}

func (h *ResourceHeader) unpack(msg []byte, off int) (int, error) {
    newOff := off
    var err error
    if newOff, err = h.Name.unpack(msg, newOff); err != nil {
        return off, &nestedError{"Name", err}
    }
    if h.Type, newOff, err = unpackType(msg, newOff); err != nil {
        return off, &nestedError{"Type", err}
    }
    if h.Class, newOff, err = unpackClass(msg, newOff); err != nil {
        return off, &nestedError{"Class", err}
    }
    if h.TTL, newOff, err = unpackUint32(msg, newOff); err != nil {
        return off, &nestedError{"TTL", err}
    }
    if h.Length, newOff, err = unpackUint16(msg, newOff); err != nil {
        return off, &nestedError{"Length", err}
    }
    return newOff, nil
}

// fixLen updates a packed ResourceHeader to include the length of the
// ResourceBody.
//
// lenOff is the offset of the ResourceHeader.Length field in msg.
//
// preLen is the length that msg was before the ResourceBody was packed.
func (h *ResourceHeader) fixLen(msg []byte, lenOff int, preLen int) error {
    conLen := len(msg) - preLen
    if conLen > int(^uint16(0)) {
        return errResTooLong
    }

    // Fill in the length now that we know how long the content is.
    packUint16(msg[lenOff:lenOff], uint16(conLen))
    h.Length = uint16(conLen)

    return nil
}

// EDNS(0) wire constants.
const (
    edns0Version = 0

    edns0DNSSECOK     = 0x00008000
    ednsVersionMask   = 0x00ff0000
    edns0DNSSECOKMask = 0x00ff8000
)

// SetEDNS0 configures h for EDNS(0).
//
// The provided extRCode must be an extedned RCode.
func (h *ResourceHeader) SetEDNS0(udpPayloadLen int, extRCode RCode, dnssecOK bool) error {
    h.Name = Name{Data: [nameLen]byte{'.'}, Length: 1} // RFC 6891 section 6.1.2
    h.Type = TypeOPT
    h.Class = Class(udpPayloadLen)
    h.TTL = uint32(extRCode) >> 4 << 24
    if dnssecOK {
        h.TTL |= edns0DNSSECOK
    }
    return nil
}

// DNSSECAllowed reports whether the DNSSEC OK bit is set.
func (h *ResourceHeader) DNSSECAllowed() bool {
    return h.TTL&edns0DNSSECOKMask == edns0DNSSECOK // RFC 6891 section 6.1.3
}

// ExtendedRCode returns an extended RCode.
//
// The provided rcode must be the RCode in DNS message header.
func (h *ResourceHeader) ExtendedRCode(rcode RCode) RCode {
    if h.TTL&ednsVersionMask == edns0Version { // RFC 6891 section 6.1.3
        return RCode(h.TTL>>24<<4) | rcode
    }
    return rcode
}
*/

// A ResourceBody is a DNS resource record minus the header.
pub trait ResourceBody: fmt::Display {
    // pack packs a Resource except for its header.
    fn pack(
        &self,
        msg: &[u8],
        compression: &mut HashMap<String, usize>,
        compression_off: usize,
    ) -> Result<Vec<u8>, Error>;

    // real_type returns the actual type of the Resource. This is used to
    // fill in the header Type field.
    fn real_type(&self) -> DNSType;
}

/*
// pack appends the wire format of the Resource to msg.
func (r *Resource) pack(msg []byte, compression map[string]int, compressionOff int) ([]byte, error) {
    if r.Body == nil {
        return msg, errNilResouceBody
    }
    oldMsg := msg
    r.Header.Type = r.Body.real_type()
    msg, lenOff, err := r.Header.pack(msg, compression, compressionOff)
    if err != nil {
        return msg, &nestedError{"ResourceHeader", err}
    }
    preLen := len(msg)
    msg, err = r.Body.pack(msg, compression, compressionOff)
    if err != nil {
        return msg, &nestedError{"content", err}
    }
    if err := r.Header.fixLen(msg, lenOff, preLen); err != nil {
        return oldMsg, err
    }
    return msg, nil
}

func unpackResourceBody(msg []byte, off int, hdr ResourceHeader) (ResourceBody, int, error) {
    var (
        r    ResourceBody
        err  error
        name string
    )
    switch hdr.Type {
    case TypeA:
        var rb AResource
        rb, err = unpackAResource(msg, off)
        r = &rb
        name = "A"
    case TypeNS:
        var rb NSResource
        rb, err = unpackNSResource(msg, off)
        r = &rb
        name = "NS"
    case TypeCNAME:
        var rb CNAMEResource
        rb, err = unpackCNAMEResource(msg, off)
        r = &rb
        name = "CNAME"
    case TypeSOA:
        var rb SOAResource
        rb, err = unpackSOAResource(msg, off)
        r = &rb
        name = "SOA"
    case TypePTR:
        var rb PTRResource
        rb, err = unpackPTRResource(msg, off)
        r = &rb
        name = "PTR"
    case TypeMX:
        var rb MXResource
        rb, err = unpackMXResource(msg, off)
        r = &rb
        name = "MX"
    case TypeTXT:
        var rb TXTResource
        rb, err = unpackTXTResource(msg, off, hdr.Length)
        r = &rb
        name = "TXT"
    case TypeAAAA:
        var rb AAAAResource
        rb, err = unpackAAAAResource(msg, off)
        r = &rb
        name = "AAAA"
    case TypeSRV:
        var rb SRVResource
        rb, err = unpackSRVResource(msg, off)
        r = &rb
        name = "SRV"
    case TypeOPT:
        var rb OPTResource
        rb, err = unpackOPTResource(msg, off, hdr.Length)
        r = &rb
        name = "OPT"
    }
    if err != nil {
        return nil, off, &nestedError{name + " record", err}
    }
    if r == nil {
        return nil, off, errors.New("invalid resource type: " + hdr.Type.String())
    }
    return r, off + int(hdr.Length), nil
}
*/
