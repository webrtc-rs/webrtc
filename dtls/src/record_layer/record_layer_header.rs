use crate::content::*;

/*
const (
    recordLayerHeaderSize = 13
    maxSequenceNumber     = 0x0000FFFFFFFFFFFF

    dtls1_2Major = 0xfe
    dtls1_2Minor = 0xfd

    dtls1_0Major = 0xfe
    dtls1_0Minor = 0xff

    // VersionDTLS12 is the DTLS version in the same style as
    // VersionTLSXX from crypto/tls
    VersionDTLS12 = 0xfefd
)

var protocolVersion1_0 = ProtocolVersion{dtls1_0Major, dtls1_0Minor}
var protocolVersion1_2 = ProtocolVersion{dtls1_2Major, dtls1_2Minor}
*/
// https://tools.ietf.org/html/rfc4346#section-6.2.1
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct ProtocolVersion {
    pub major: u8,
    pub minor: u8,
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct RecordLayerHeader {
    content_type: ContentType,
    content_len: u16,
    protocol_version: ProtocolVersion,
    epoch: u16,
    sequence_number: u64, // uint48 in spec
}

/*
func (v ProtocolVersion) Equal(x ProtocolVersion) bool {
    return v.major == x.major && v.minor == x.minor
}

func (r *RecordLayerHeader) Marshal() ([]byte, error) {
    if r.sequence_number > maxSequenceNumber {
        return nil, errSequenceNumberOverflow
    }

    out := make([]byte, recordLayerHeaderSize)
    out[0] = byte(r.content_type)
    out[1] = r.ProtocolVersion.major
    out[2] = r.ProtocolVersion.minor
    binary.BigEndian.PutUint16(out[3:], r.epoch)
    putBigEndianUint48(out[5:], r.sequence_number)
    binary.BigEndian.PutUint16(out[recordLayerHeaderSize-2:], r.content_len)
    return out, nil
}

func (r *RecordLayerHeader) Unmarshal(data []byte) error {
    if len(data) < recordLayerHeaderSize {
        return errBufferTooSmall
    }
    r.content_type = content_type(data[0])
    r.ProtocolVersion.major = data[1]
    r.ProtocolVersion.minor = data[2]
    r.epoch = binary.BigEndian.Uint16(data[3:])

    // SequenceNumber is stored as uint48, make into uint64
    seqCopy := make([]byte, 8)
    copy(seqCopy[2:], data[5:11])
    r.sequence_number = binary.BigEndian.Uint64(seqCopy)

    if !r.ProtocolVersion.Equal(protocolVersion1_0) && !r.ProtocolVersion.Equal(protocolVersion1_2) {
        return errUnsupportedProtocolVersion
    }

    return nil
}
*/
