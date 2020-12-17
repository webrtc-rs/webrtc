use crate::agent::*;
use crate::attributes::*;
use crate::errors::*;

use util::Error;

use std::fmt;

// MAGIC_COOKIE is fixed value that aids in distinguishing STUN packets
// from packets of other protocols when STUN is multiplexed with those
// other protocols on the same Port.
//
// The magic cookie field MUST contain the fixed value 0x2112A442 in
// network byte order.
//
// Defined in "STUN Message Structure", section 6.
const MAGIC_COOKIE: u32 = 0x2112A442;
const ATTRIBUTE_HEADER_SIZE: usize = 4;
const MESSAGE_HEADER_SIZE: usize = 20;

// TRANSACTION_ID_SIZE is length of transaction id array (in bytes).
pub const TRANSACTION_ID_SIZE: usize = 12; // 96 bit

// is_message returns true if b looks like STUN message.
// Useful for multiplexing. is_message does not guarantee
// that decoding will be successful.
pub fn is_message(b: &[u8]) -> bool {
    b.len() >= MESSAGE_HEADER_SIZE && u32::from_be_bytes([b[4], b[5], b[6], b[7]]) == MAGIC_COOKIE
}
// Message represents a single STUN packet. It uses aggressive internal
// buffering to enable zero-allocation encoding and decoding,
// so there are some usage constraints:
//
// 	Message, its fields, results of m.Get or any attribute a.GetFrom
//	are valid only until Message.Raw is not modified.
#[derive(Default)]
pub struct Message {
    pub typ: MessageType,
    pub length: u32, // len(Raw) not including header
    pub transaction_id: TransactionId,
    pub attributes: Attributes,
    pub raw: Vec<u8>,
}

const DEFAULT_RAW_CAPACITY: usize = 120;

impl Message {
    // New returns *Message with pre-allocated Raw.
    pub fn new() -> Self {
        Message {
            raw: {
                let mut raw = Vec::with_capacity(DEFAULT_RAW_CAPACITY);
                raw.extend_from_slice(&[0; MESSAGE_HEADER_SIZE]);
                raw
            },
            ..Default::default()
        }
    }

    // Decode decodes m.Raw into m.
    pub fn decode(&mut self) -> Result<(), Error> {
        // decoding message header
        let buf = &self.raw;
        if buf.len() < MESSAGE_HEADER_SIZE {
            return Err(ERR_UNEXPECTED_HEADER_EOF.clone());
        }

        let t = u16::from_be_bytes([buf[0], buf[1]]); // first 2 bytes
        let size = u16::from_be_bytes([buf[2], buf[3]]) as usize; // second 2 bytes
        let cookie = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]); // last 4 bytes
        let full_size = MESSAGE_HEADER_SIZE + size; // len(m.Raw)

        if cookie != MAGIC_COOKIE {
            return Err(Error::new(format!(
                "{:x} is invalid magic cookie (should be {:x})",
                cookie, MAGIC_COOKIE
            )));
        }
        if buf.len() < full_size {
            return Err(Error::new(format!(
                "buffer length {} is less than {} (expected message size)",
                buf.len(),
                full_size
            )));
        }

        // saving header data
        self.typ.read_value(t);
        self.length = size as u32;
        self.transaction_id
            .0
            .copy_from_slice(&buf[8..MESSAGE_HEADER_SIZE]);

        self.attributes = Attributes(vec![]);
        let mut offset = 0;
        let mut b = &buf[MESSAGE_HEADER_SIZE..full_size];

        while offset < size {
            // checking that we have enough bytes to read header
            if b.len() < ATTRIBUTE_HEADER_SIZE {
                return Err(Error::new(format!(
                    "buffer length {} is less than {} (expected header size)",
                    b.len(),
                    ATTRIBUTE_HEADER_SIZE
                )));
            }

            let mut a = RawAttribute {
                typ: compat_attr_type(u16::from_be_bytes([b[0], b[1]])), // first 2 bytes
                length: u16::from_be_bytes([b[2], b[3]]),                // second 2 bytes
                ..Default::default()
            };
            let a_l = a.length as usize; // attribute length
            let a_buff_l = nearest_padded_value_length(a_l); // expected buffer length (with padding)

            b = &b[ATTRIBUTE_HEADER_SIZE..]; // slicing again to simplify value read
            offset += ATTRIBUTE_HEADER_SIZE;
            if b.len() < a_buff_l {
                // checking size
                return Err(Error::new(format!(
                    "buffer length {} is less than {} (expected value size for {})",
                    b.len(),
                    a_buff_l,
                    a.typ
                )));
            }
            a.value = b[..a_l].to_vec();
            offset += a_buff_l;
            b = &b[a_buff_l..];

            self.attributes.0.push(a);
        }

        Ok(())
    }

    // marshal_binary implements the encoding.BinaryMarshaler interface.
    pub fn marshal_binary(&self) -> Result<Vec<u8>, Error> {
        // We can't return m.Raw, allocation is expected by implicit interface
        // contract induced by other implementations.
        Ok(self.raw.clone())
    }

    // unmarshal_binary implements the encoding.BinaryUnmarshaler interface.
    pub fn unmarshal_binary(&mut self, data: &[u8]) -> Result<(), Error> {
        // We can't retain data, copy is expected by interface contract.
        self.raw = vec![];
        self.raw.extend_from_slice(data);
        self.decode()
    }
}

/*
// AddTo sets b.TransactionID to m.TransactionID.
//
// Implements Setter to aid in crafting responses.
func (m *Message) AddTo(b *Message) error {
    b.TransactionID = m.TransactionID
    b.WriteTransactionID()
    return nil
}

// NewTransactionID sets m.TransactionID to random value from crypto/rand
// and returns error if any.
func (m *Message) NewTransactionID() error {
    _, err := io.ReadFull(rand.Reader, m.TransactionID[:])
    if err == nil {
        m.WriteTransactionID()
    }
    return err
}

func (m *Message) String() string {
    tID := base64.StdEncoding.EncodeToString(m.TransactionID[:])
    return fmt.Sprintf("%s l=%d attrs=%d id=%s", m.Type, m.Length, len(m.Attributes), tID)
}

// Reset resets Message, attributes and underlying buffer length.
func (m *Message) Reset() {
    m.Raw = m.Raw[:0]
    m.Length = 0
    m.Attributes = m.Attributes[:0]
}

// grow ensures that internal buffer has n length.
func (m *Message) grow(n int) {
    if len(m.Raw) >= n {
        return
    }
    if cap(m.Raw) >= n {
        m.Raw = m.Raw[:n]
        return
    }
    m.Raw = append(m.Raw, make([]byte, n-len(m.Raw))...)
}

// Add appends new attribute to message. Not goroutine-safe.
//
// Value of attribute is copied to internal buffer so
// it is safe to reuse v.
func (m *Message) Add(t AttrType, v []byte) {
    // Allocating buffer for TLV (type-length-value).
    // T = t, L = len(v), V = v.
    // m.Raw will look like:
    // [0:20]                               <- message header
    // [20:20+m.Length]                     <- existing message attributes
    // [20+m.Length:20+m.Length+len(v) + 4] <- allocated buffer for new TLV
    // [first:last]                         <- same as previous
    // [0 1|2 3|4    4 + len(v)]            <- mapping for allocated buffer
    //   T   L        V
    allocSize := ATTRIBUTE_HEADER_SIZE + len(v)  // ~ len(TLV) = len(TL) + len(V)
    first := MESSAGE_HEADER_SIZE + int(m.Length) // first byte number
    last := first + allocSize                  // last byte number
    m.grow(last)                               // growing cap(Raw) to fit TLV
    m.Raw = m.Raw[:last]                       // now len(Raw) = last
    m.Length += uint32(allocSize)              // rendering length change

    // Sub-slicing internal buffer to simplify encoding.
    buf := m.Raw[first:last]           // slice for TLV
    value := buf[ATTRIBUTE_HEADER_SIZE:] // slice for V
    attr := RawAttribute{
        Type:   t,              // T
        Length: uint16(len(v)), // L
        Value:  value,          // V
    }

    // Encoding attribute TLV to allocated buffer.
    bin.PutUint16(buf[0:2], attr.Type.Value()) // T
    bin.PutUint16(buf[2:4], attr.Length)       // L
    copy(value, v)                             // V

    // Checking that attribute value needs padding.
    if attr.Length%padding != 0 {
        // Performing padding.
        bytesToAdd := nearestPaddedValueLength(len(v)) - len(v)
        last += bytesToAdd
        m.grow(last)
        // setting all padding bytes to zero
        // to prevent data leak from previous
        // data in next bytesToAdd bytes
        buf = m.Raw[last-bytesToAdd : last]
        for i := range buf {
            buf[i] = 0
        }
        m.Raw = m.Raw[:last]           // increasing buffer length
        m.Length += uint32(bytesToAdd) // rendering length change
    }
    m.Attributes = append(m.Attributes, attr)
    m.WriteLength()
}

func attrSliceEqual(a, b Attributes) bool {
    for _, attr := range a {
        found := false
        for _, attrB := range b {
            if attrB.Type != attr.Type {
                continue
            }
            if attrB.Equal(attr) {
                found = true
                break
            }
        }
        if !found {
            return false
        }
    }
    return true
}

func attrEqual(a, b Attributes) bool {
    if a == nil && b == nil {
        return true
    }
    if a == nil || b == nil {
        return false
    }
    if len(a) != len(b) {
        return false
    }
    if !attrSliceEqual(a, b) {
        return false
    }
    if !attrSliceEqual(b, a) {
        return false
    }
    return true
}

// Equal returns true if Message b equals to m.
// Ignores m.Raw.
func (m *Message) Equal(b *Message) bool {
    if m == nil && b == nil {
        return true
    }
    if m == nil || b == nil {
        return false
    }
    if m.Type != b.Type {
        return false
    }
    if m.TransactionID != b.TransactionID {
        return false
    }
    if m.Length != b.Length {
        return false
    }
    if !attrEqual(m.Attributes, b.Attributes) {
        return false
    }
    return true
}

// WriteLength writes m.Length to m.Raw.
func (m *Message) WriteLength() {
    m.grow(4)
    bin.PutUint16(m.Raw[2:4], uint16(m.Length))
}

// WriteHeader writes header to underlying buffer. Not goroutine-safe.
func (m *Message) WriteHeader() {
    m.grow(MESSAGE_HEADER_SIZE)
    _ = m.Raw[:MESSAGE_HEADER_SIZE] // early bounds check to guarantee safety of writes below

    m.WriteType()
    m.WriteLength()
    bin.PutUint32(m.Raw[4:8], MAGIC_COOKIE)               // magic cookie
    copy(m.Raw[8:MESSAGE_HEADER_SIZE], m.TransactionID[:]) // transaction ID
}

// WriteTransactionID writes m.TransactionID to m.Raw.
func (m *Message) WriteTransactionID() {
    copy(m.Raw[8:MESSAGE_HEADER_SIZE], m.TransactionID[:]) // transaction ID
}

// WriteAttributes encodes all m.Attributes to m.
func (m *Message) WriteAttributes() {
    attributes := m.Attributes
    m.Attributes = attributes[:0]
    for _, a := range attributes {
        m.Add(a.Type, a.Value)
    }
    m.Attributes = attributes
}

// WriteType writes m.Type to m.Raw.
func (m *Message) WriteType() {
    m.grow(2)
    bin.PutUint16(m.Raw[0:2], m.Type.Value()) // message type
}

// SetType sets m.Type and writes it to m.Raw.
func (m *Message) SetType(t MessageType) {
    m.Type = t
    m.WriteType()
}

// Encode re-encodes message into m.Raw.
func (m *Message) Encode() {
    m.Raw = m.Raw[:0]
    m.WriteHeader()
    m.Length = 0
    m.WriteAttributes()
}

// WriteTo implements WriterTo via calling Write(m.Raw) on w and returning
// call result.
func (m *Message) WriteTo(w io.Writer) (int64, error) {
    n, err := w.Write(m.Raw)
    return int64(n), err
}

// ReadFrom implements ReaderFrom. Reads message from r into m.Raw,
// Decodes it and return error if any. If m.Raw is too small, will return
// ErrUnexpectedEOF, ErrUnexpectedHeaderEOF or *DecodeErr.
//
// Can return *DecodeErr while decoding too.
func (m *Message) ReadFrom(r io.Reader) (int64, error) {
    tBuf := m.Raw[:cap(m.Raw)]
    var (
        n   int
        err error
    )
    if n, err = r.Read(tBuf); err != nil {
        return int64(n), err
    }
    m.Raw = tBuf[:n]
    return int64(n), m.Decode()
}



// Write decodes message and return error if any.
//
// Any error is unrecoverable, but message could be partially decoded.
func (m *Message) Write(tBuf []byte) (int, error) {
    m.Raw = append(m.Raw[:0], tBuf...)
    return len(tBuf), m.Decode()
}

// CloneTo clones m to b securing any further m mutations.
func (m *Message) CloneTo(b *Message) error {
    b.Raw = append(b.Raw[:0], m.Raw...)
    return b.Decode()
}


// Contains return true if message contain t attribute.
func (m *Message) Contains(t AttrType) bool {
    for _, a := range m.Attributes {
        if a.Type == t {
            return true
        }
    }
    return false
}


*/
// MessageClass is 8-bit representation of 2-bit class of STUN Message Class.
#[derive(Default, PartialEq, Eq)]
pub struct MessageClass(u8);

// Possible values for message class in STUN Message Type.
pub const CLASS_REQUEST: MessageClass = MessageClass(0x00); // 0b00
pub const CLASS_INDICATION: MessageClass = MessageClass(0x01); // 0b01
pub const CLASS_SUCCESS_RESPONSE: MessageClass = MessageClass(0x02); // 0b10
pub const CLASS_ERROR_RESPONSE: MessageClass = MessageClass(0x03); // 0b11

impl fmt::Display for MessageClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            CLASS_REQUEST => "request",
            CLASS_INDICATION => "indication",
            CLASS_SUCCESS_RESPONSE => "success response",
            CLASS_ERROR_RESPONSE => "error response",
            _ => "unknown message class",
        };

        write!(f, "{}", s)
    }
}

// Method is uint16 representation of 12-bit STUN method.
#[derive(Default, PartialEq, Eq)]
pub struct Method(u16);

// Possible methods for STUN Message.
pub const METHOD_BINDING: Method = Method(0x001);
pub const METHOD_ALLOCATE: Method = Method(0x003);
pub const METHOD_REFRESH: Method = Method(0x004);
pub const METHOD_SEND: Method = Method(0x006);
pub const METHOD_DATA: Method = Method(0x007);
pub const METHOD_CREATE_PERMISSION: Method = Method(0x008);
pub const METHOD_CHANNEL_BIND: Method = Method(0x009);

// Methods from RFC 6062.
pub const METHOD_CONNECT: Method = Method(0x000a);
pub const METHOD_CONNECTION_BIND: Method = Method(0x000b);
pub const METHOD_CONNECTION_ATTEMPT: Method = Method(0x000c);

impl fmt::Display for Method {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let unknown = format!("0x{:x}", self.0);

        let s = match *self {
            METHOD_BINDING => "Binding",
            METHOD_ALLOCATE => "Allocate",
            METHOD_REFRESH => "Refresh",
            METHOD_SEND => "Send",
            METHOD_DATA => "Data",
            METHOD_CREATE_PERMISSION => "CreatePermission",
            METHOD_CHANNEL_BIND => "ChannelBind",

            // RFC 6062.
            METHOD_CONNECT => "Connect",
            METHOD_CONNECTION_BIND => "ConnectionBind",
            METHOD_CONNECTION_ATTEMPT => "ConnectionAttempt",
            _ => unknown.as_str(),
        };

        write!(f, "{}", s)
    }
}

// MessageType is STUN Message Type Field.
#[derive(Default)]
pub struct MessageType {
    pub method: Method,      // e.g. binding
    pub class: MessageClass, // e.g. request
}

// Common STUN message types.
// Binding request message type.
pub const BINDING_REQUEST: MessageType = MessageType {
    method: METHOD_BINDING,
    class: CLASS_REQUEST,
};
// Binding success response message type
pub const BINDING_SUCCESS: MessageType = MessageType {
    method: METHOD_BINDING,
    class: CLASS_SUCCESS_RESPONSE,
};
// Binding error response message type.
pub const BINDING_ERROR: MessageType = MessageType {
    method: METHOD_BINDING,
    class: CLASS_ERROR_RESPONSE,
};

impl fmt::Display for MessageType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.method.0, self.class.0)
    }
}

/*TODO:
// AddTo sets m type to t.
func (t MessageType) AddTo(m *Message) error {
    m.SetType(t)
    return nil
}
*/

const METHOD_ABITS: u16 = 0xf; // 0b0000000000001111
const METHOD_BBITS: u16 = 0x70; // 0b0000000001110000
const METHOD_DBITS: u16 = 0xf80; // 0b0000111110000000

const METHOD_BSHIFT: u16 = 1;
const METHOD_DSHIFT: u16 = 2;

const FIRST_BIT: u16 = 0x1;
const SECOND_BIT: u16 = 0x2;

const C0BIT: u16 = FIRST_BIT;
const C1BIT: u16 = SECOND_BIT;

const CLASS_C0SHIFT: u16 = 4;
const CLASS_C1SHIFT: u16 = 7;

impl MessageType {
    // NewType returns new message type with provided method and class.
    pub fn new(method: Method, class: MessageClass) -> Self {
        MessageType { method, class }
    }

    // Value returns bit representation of messageType.
    pub fn value(&self) -> u16 {
        //	 0                 1
        //	 2  3  4 5 6 7 8 9 0 1 2 3 4 5
        //	+--+--+-+-+-+-+-+-+-+-+-+-+-+-+
        //	|M |M |M|M|M|C|M|M|M|C|M|M|M|M|
        //	|11|10|9|8|7|1|6|5|4|0|3|2|1|0|
        //	+--+--+-+-+-+-+-+-+-+-+-+-+-+-+
        // Figure 3: Format of STUN Message Type Field

        // Warning: Abandon all hope ye who enter here.
        // Splitting M into A(M0-M3), B(M4-M6), D(M7-M11).
        let method = self.method.0;
        let a = method & METHOD_ABITS; // A = M * 0b0000000000001111 (right 4 bits)
        let b = method & METHOD_BBITS; // B = M * 0b0000000001110000 (3 bits after A)
        let d = method & METHOD_DBITS; // D = M * 0b0000111110000000 (5 bits after B)

        // Shifting to add "holes" for C0 (at 4 bit) and C1 (8 bit).
        let method = a + (b << METHOD_BSHIFT) + (d << METHOD_DSHIFT);

        // C0 is zero bit of C, C1 is first bit.
        // C0 = C * 0b01, C1 = (C * 0b10) >> 1
        // Ct = C0 << 4 + C1 << 8.
        // Optimizations: "((C * 0b10) >> 1) << 8" as "(C * 0b10) << 7"
        // We need C0 shifted by 4, and C1 by 8 to fit "11" and "7" positions
        // (see figure 3).
        let c = self.class.0 as u16;
        let c0 = (c & C0BIT) << CLASS_C0SHIFT;
        let c1 = (c & C1BIT) << CLASS_C1SHIFT;
        let class = c0 + c1;

        method + class
    }

    // ReadValue decodes uint16 into MessageType.
    pub fn read_value(&mut self, value: u16) {
        // Decoding class.
        // We are taking first bit from v >> 4 and second from v >> 7.
        let c0 = (value >> CLASS_C0SHIFT) & C0BIT;
        let c1 = (value >> CLASS_C1SHIFT) & C1BIT;
        let class = c0 + c1;
        self.class = MessageClass(class as u8);

        // Decoding method.
        let a = value & METHOD_ABITS; // A(M0-M3)
        let b = (value >> METHOD_BSHIFT) & METHOD_BBITS; // B(M4-M6)
        let d = (value >> METHOD_DSHIFT) & METHOD_DBITS; // D(M7-M11)
        let m = a + b + d;
        self.method = Method(m);
    }
}
