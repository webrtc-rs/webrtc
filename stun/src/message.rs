#[cfg(test)]
mod message_test;

use std::fmt;
use std::io::{Read, Write};

use base64::prelude::BASE64_STANDARD;
use base64::Engine;
use rand::Rng;

use crate::agent::*;
use crate::attributes::*;
use crate::error::*;

// MAGIC_COOKIE is fixed value that aids in distinguishing STUN packets
// from packets of other protocols when STUN is multiplexed with those
// other protocols on the same Port.
//
// The magic cookie field MUST contain the fixed value 0x2112A442 in
// network byte order.
//
// Defined in "STUN Message Structure", section 6.
pub const MAGIC_COOKIE: u32 = 0x2112A442;
pub const ATTRIBUTE_HEADER_SIZE: usize = 4;
pub const MESSAGE_HEADER_SIZE: usize = 20;

// TRANSACTION_ID_SIZE is length of transaction id array (in bytes).
pub const TRANSACTION_ID_SIZE: usize = 12; // 96 bit

// Interfaces that are implemented by message attributes, shorthands for them,
// or helpers for message fields as type or transaction id.
pub trait Setter {
    // Setter sets *Message attribute.
    fn add_to(&self, m: &mut Message) -> Result<()>;
}

// Getter parses attribute from *Message.
pub trait Getter {
    fn get_from(&mut self, m: &Message) -> Result<()>;
}

// Checker checks *Message attribute.
pub trait Checker {
    fn check(&self, m: &Message) -> Result<()>;
}

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
#[derive(Default, Debug, Clone)]
pub struct Message {
    pub typ: MessageType,
    pub length: u32, // len(Raw) not including header
    pub transaction_id: TransactionId,
    pub attributes: Attributes,
    pub raw: Vec<u8>,
}

impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let t_id = BASE64_STANDARD.encode(self.transaction_id.0);
        write!(
            f,
            "{} l={} attrs={} id={}",
            self.typ,
            self.length,
            self.attributes.0.len(),
            t_id
        )
    }
}

// Equal returns true if Message b equals to m.
// Ignores m.Raw.
impl PartialEq for Message {
    fn eq(&self, other: &Self) -> bool {
        if self.typ != other.typ {
            return false;
        }
        if self.transaction_id != other.transaction_id {
            return false;
        }
        if self.length != other.length {
            return false;
        }
        if self.attributes != other.attributes {
            return false;
        }
        true
    }
}

const DEFAULT_RAW_CAPACITY: usize = 120;

impl Setter for Message {
    // add_to sets b.TransactionID to m.TransactionID.
    //
    // Implements Setter to aid in crafting responses.
    fn add_to(&self, b: &mut Message) -> Result<()> {
        b.transaction_id = self.transaction_id;
        b.write_transaction_id();
        Ok(())
    }
}

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

    // marshal_binary implements the encoding.BinaryMarshaler interface.
    pub fn marshal_binary(&self) -> Result<Vec<u8>> {
        // We can't return m.Raw, allocation is expected by implicit interface
        // contract induced by other implementations.
        Ok(self.raw.clone())
    }

    // unmarshal_binary implements the encoding.BinaryUnmarshaler interface.
    pub fn unmarshal_binary(&mut self, data: &[u8]) -> Result<()> {
        // We can't retain data, copy is expected by interface contract.
        self.raw.clear();
        self.raw.extend_from_slice(data);
        self.decode()
    }

    // NewTransactionID sets m.TransactionID to random value from crypto/rand
    // and returns error if any.
    pub fn new_transaction_id(&mut self) -> Result<()> {
        rand::thread_rng().fill(&mut self.transaction_id.0);
        self.write_transaction_id();
        Ok(())
    }

    // Reset resets Message, attributes and underlying buffer length.
    pub fn reset(&mut self) {
        self.raw.clear();
        self.length = 0;
        self.attributes.0.clear();
    }

    // grow ensures that internal buffer has n length.
    fn grow(&mut self, n: usize, resize: bool) {
        if self.raw.len() >= n {
            if resize {
                self.raw.resize(n, 0);
            }
            return;
        }
        self.raw.extend_from_slice(&vec![0; n - self.raw.len()]);
    }

    // Add appends new attribute to message. Not goroutine-safe.
    //
    // Value of attribute is copied to internal buffer so
    // it is safe to reuse v.
    pub fn add(&mut self, t: AttrType, v: &[u8]) {
        // Allocating buffer for TLV (type-length-value).
        // T = t, L = len(v), V = v.
        // m.Raw will look like:
        // [0:20]                               <- message header
        // [20:20+m.Length]                     <- existing message attributes
        // [20+m.Length:20+m.Length+len(v) + 4] <- allocated buffer for new TLV
        // [first:last]                         <- same as previous
        // [0 1|2 3|4    4 + len(v)]            <- mapping for allocated buffer
        //   T   L        V
        let alloc_size = ATTRIBUTE_HEADER_SIZE + v.len(); // ~ len(TLV) = len(TL) + len(V)
        let first = MESSAGE_HEADER_SIZE + self.length as usize; // first byte number
        let mut last = first + alloc_size; // last byte number
        self.grow(last, true); // growing cap(Raw) to fit TLV
        self.length += alloc_size as u32; // rendering length change

        // Encoding attribute TLV to allocated buffer.
        let buf = &mut self.raw[first..last];
        buf[0..2].copy_from_slice(&t.value().to_be_bytes()); // T
        buf[2..4].copy_from_slice(&(v.len() as u16).to_be_bytes()); // L

        let value = &mut buf[ATTRIBUTE_HEADER_SIZE..];
        value.copy_from_slice(v); // V

        let attr = RawAttribute {
            typ: t,                 // T
            length: v.len() as u16, // L
            value: value.to_vec(),  // V
        };

        // Checking that attribute value needs padding.
        if attr.length as usize % PADDING != 0 {
            // Performing padding.
            let bytes_to_add = nearest_padded_value_length(v.len()) - v.len();
            last += bytes_to_add;
            self.grow(last, true);
            // setting all padding bytes to zero
            // to prevent data leak from previous
            // data in next bytes_to_add bytes
            let buf = &mut self.raw[last - bytes_to_add..last];
            for b in buf {
                *b = 0;
            }
            self.length += bytes_to_add as u32; // rendering length change
        }
        self.attributes.0.push(attr);
        self.write_length();
    }

    // WriteLength writes m.Length to m.Raw.
    pub fn write_length(&mut self) {
        self.grow(4, false);
        self.raw[2..4].copy_from_slice(&(self.length as u16).to_be_bytes());
    }

    // WriteHeader writes header to underlying buffer. Not goroutine-safe.
    pub fn write_header(&mut self) {
        self.grow(MESSAGE_HEADER_SIZE, false);

        self.write_type();
        self.write_length();
        self.raw[4..8].copy_from_slice(&MAGIC_COOKIE.to_be_bytes()); // magic cookie
        self.raw[8..MESSAGE_HEADER_SIZE].copy_from_slice(&self.transaction_id.0);
        // transaction ID
    }

    // WriteTransactionID writes m.TransactionID to m.Raw.
    pub fn write_transaction_id(&mut self) {
        self.raw[8..MESSAGE_HEADER_SIZE].copy_from_slice(&self.transaction_id.0);
        // transaction ID
    }

    // WriteAttributes encodes all m.Attributes to m.
    pub fn write_attributes(&mut self) {
        let attributes: Vec<RawAttribute> = self.attributes.0.drain(..).collect();
        for a in &attributes {
            self.add(a.typ, &a.value);
        }
        self.attributes = Attributes(attributes);
    }

    // WriteType writes m.Type to m.Raw.
    pub fn write_type(&mut self) {
        self.grow(2, false);
        self.raw[..2].copy_from_slice(&self.typ.value().to_be_bytes()); // message type
    }

    // SetType sets m.Type and writes it to m.Raw.
    pub fn set_type(&mut self, t: MessageType) {
        self.typ = t;
        self.write_type();
    }

    // Encode re-encodes message into m.Raw.
    pub fn encode(&mut self) {
        self.raw.clear();
        self.write_header();
        self.length = 0;
        self.write_attributes();
    }

    // Decode decodes m.Raw into m.
    pub fn decode(&mut self) -> Result<()> {
        // decoding message header
        let buf = &self.raw;
        if buf.len() < MESSAGE_HEADER_SIZE {
            return Err(Error::ErrUnexpectedHeaderEof);
        }

        let t = u16::from_be_bytes([buf[0], buf[1]]); // first 2 bytes
        let size = u16::from_be_bytes([buf[2], buf[3]]) as usize; // second 2 bytes
        let cookie = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]); // last 4 bytes
        let full_size = MESSAGE_HEADER_SIZE + size; // len(m.Raw)

        if cookie != MAGIC_COOKIE {
            return Err(Error::Other(format!(
                "{cookie:x} is invalid magic cookie (should be {MAGIC_COOKIE:x})"
            )));
        }
        if buf.len() < full_size {
            return Err(Error::Other(format!(
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

        self.attributes.0.clear();
        let mut offset = 0;
        let mut b = &buf[MESSAGE_HEADER_SIZE..full_size];

        while offset < size {
            // checking that we have enough bytes to read header
            if b.len() < ATTRIBUTE_HEADER_SIZE {
                return Err(Error::Other(format!(
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
                return Err(Error::Other(format!(
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

    // WriteTo implements WriterTo via calling Write(m.Raw) on w and returning
    // call result.
    pub fn write_to<W: Write>(&self, writer: &mut W) -> Result<usize> {
        let n = writer.write(&self.raw)?;
        Ok(n)
    }

    // ReadFrom implements ReaderFrom. Reads message from r into m.Raw,
    // Decodes it and return error if any. If m.Raw is too small, will return
    // ErrUnexpectedEOF, ErrUnexpectedHeaderEOF or *DecodeErr.
    //
    // Can return *DecodeErr while decoding too.
    pub fn read_from<R: Read>(&mut self, reader: &mut R) -> Result<usize> {
        let mut t_buf = vec![0; DEFAULT_RAW_CAPACITY];
        let n = reader.read(&mut t_buf)?;
        self.raw = t_buf[..n].to_vec();
        self.decode()?;
        Ok(n)
    }

    // Write decodes message and return error if any.
    //
    // Any error is unrecoverable, but message could be partially decoded.
    pub fn write(&mut self, t_buf: &[u8]) -> Result<usize> {
        self.raw.clear();
        self.raw.extend_from_slice(t_buf);
        self.decode()?;
        Ok(t_buf.len())
    }

    // CloneTo clones m to b securing any further m mutations.
    pub fn clone_to(&self, b: &mut Message) -> Result<()> {
        b.raw.clear();
        b.raw.extend_from_slice(&self.raw);
        b.decode()
    }

    // Contains return true if message contain t attribute.
    pub fn contains(&self, t: AttrType) -> bool {
        for a in &self.attributes.0 {
            if a.typ == t {
                return true;
            }
        }
        false
    }

    // get returns byte slice that represents attribute value,
    // if there is no attribute with such type,
    // ErrAttributeNotFound is returned.
    pub fn get(&self, t: AttrType) -> Result<Vec<u8>> {
        let (v, ok) = self.attributes.get(t);
        if ok {
            Ok(v.value)
        } else {
            Err(Error::ErrAttributeNotFound)
        }
    }

    // Build resets message and applies setters to it in batch, returning on
    // first error. To prevent allocations, pass pointers to values.
    //
    // Example:
    //  var (
    //  	t        = BindingRequest
    //  	username = NewUsername("username")
    //  	nonce    = NewNonce("nonce")
    //  	realm    = NewRealm("example.org")
    //  )
    //  m := new(Message)
    //  m.Build(t, username, nonce, realm)     // 4 allocations
    //  m.Build(&t, &username, &nonce, &realm) // 0 allocations
    //
    // See BenchmarkBuildOverhead.
    pub fn build(&mut self, setters: &[Box<dyn Setter>]) -> Result<()> {
        self.reset();
        self.write_header();
        for s in setters {
            s.add_to(self)?;
        }
        Ok(())
    }

    // Check applies checkers to message in batch, returning on first error.
    pub fn check<C: Checker>(&self, checkers: &[C]) -> Result<()> {
        for c in checkers {
            c.check(self)?;
        }
        Ok(())
    }

    // Parse applies getters to message in batch, returning on first error.
    pub fn parse<G: Getter>(&self, getters: &mut [G]) -> Result<()> {
        for c in getters {
            c.get_from(self)?;
        }
        Ok(())
    }
}

// MessageClass is 8-bit representation of 2-bit class of STUN Message Class.
#[derive(Default, PartialEq, Eq, Debug, Copy, Clone)]
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

        write!(f, "{s}")
    }
}

// Method is uint16 representation of 12-bit STUN method.
#[derive(Default, PartialEq, Eq, Debug, Copy, Clone)]
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

        write!(f, "{s}")
    }
}

// MessageType is STUN Message Type Field.
#[derive(Default, Debug, PartialEq, Eq, Clone, Copy)]
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
        write!(f, "{} {}", self.method, self.class)
    }
}

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

impl Setter for MessageType {
    // add_to sets m type to t.
    fn add_to(&self, m: &mut Message) -> Result<()> {
        m.set_type(*self);
        Ok(())
    }
}

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
