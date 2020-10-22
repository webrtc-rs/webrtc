use crate::content::*;

pub const RECORD_LAYER_HEADER_SIZE: usize = 13;
pub const MAX_SEQUENCE_NUMBER: u64 = 0x0000FFFFFFFFFFFF;

pub const DTLS1_2MAJOR: u8 = 0xfe;
pub const DTLS1_2MINOR: u8 = 0xfd;

pub const DTLS1_0MAJOR: u8 = 0xfe;
pub const DTLS1_0MINOR: u8 = 0xff;

// VERSION_DTLS12 is the DTLS version in the same style as
// VersionTLSXX from crypto/tls
pub const VERSION_DTLS12: u16 = 0xfefd;

pub const PROTOCOL_VERSION1_0: ProtocolVersion = ProtocolVersion {
    major: DTLS1_0MAJOR,
    minor: DTLS1_0MINOR,
};
pub const PROTOCOL_VERSION1_2: ProtocolVersion = ProtocolVersion {
    major: DTLS1_2MAJOR,
    minor: DTLS1_2MINOR,
};

// https://tools.ietf.org/html/rfc4346#section-6.2.1
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct ProtocolVersion {
    pub major: u8,
    pub minor: u8,
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct RecordLayerHeader {
    pub content_type: ContentType,
    pub content_len: u16,
    pub protocol_version: ProtocolVersion,
    pub epoch: u16,
    pub sequence_number: u64, // uint48 in spec
}

/*
func (v ProtocolVersion) Equal(x ProtocolVersion) bool {
    return v.major == x.major && v.minor == x.minor
}

func (r *RecordLayerHeader) Marshal() ([]byte, error) {
    if r.sequence_number > MAX_SEQUENCE_NUMBER {
        return nil, errSequenceNumberOverflow
    }

    out := make([]byte, RECORD_LAYER_HEADER_SIZE)
    out[0] = byte(r.content_type)
    out[1] = r.ProtocolVersion.major
    out[2] = r.ProtocolVersion.minor
    binary.BigEndian.PutUint16(out[3:], r.epoch)
    putBigEndianUint48(out[5:], r.sequence_number)
    binary.BigEndian.PutUint16(out[RECORD_LAYER_HEADER_SIZE-2:], r.content_len)
    return out, nil
}

func (r *RecordLayerHeader) Unmarshal(data []byte) error {
    if len(data) < RECORD_LAYER_HEADER_SIZE {
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

    if !r.ProtocolVersion.Equal(PROTOCOL_VERSION1_0) && !r.ProtocolVersion.Equal(PROTOCOL_VERSION1_2) {
        return errUnsupportedProtocolVersion
    }

    return nil
}
*/
