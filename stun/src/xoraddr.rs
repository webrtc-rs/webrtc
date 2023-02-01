#[cfg(test)]
mod xoraddr_test;

use crate::addr::*;
use crate::attributes::*;
use crate::checks::*;
use crate::error::*;
use crate::message::*;

use std::fmt;
use std::mem;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

const WORD_SIZE: usize = mem::size_of::<usize>();

//var supportsUnaligned = runtime.GOARCH == "386" || runtime.GOARCH == "amd64" // nolint:gochecknoglobals

// fast_xor_bytes xors in bulk. It only works on architectures that
// support unaligned read/writes.
/*TODO: fn fast_xor_bytes(dst:&[u8], a:&[u8], b:&[u8]) ->usize {
    let mut n = a.len();
    if b.len() < n {
        n = b.len();
    }

    let w = n / WORD_SIZE;
    if w > 0 {
        let dw = *(*[]uintptr)(unsafe.Pointer(&dst))
        let aw = *(*[]uintptr)(unsafe.Pointer(&a))
        let bw = *(*[]uintptr)(unsafe.Pointer(&b))
        for i := 0; i < w; i++ {
            dw[i] = aw[i] ^ bw[i]
        }
    }

    for i := n - n%WORD_SIZE; i < n; i++ {
        dst[i] = a[i] ^ b[i]
    }

    return n
}*/

fn safe_xor_bytes(dst: &mut [u8], a: &[u8], b: &[u8]) -> usize {
    let mut n = a.len();
    if b.len() < n {
        n = b.len();
    }
    if dst.len() < n {
        n = dst.len();
    }
    for i in 0..n {
        dst[i] = a[i] ^ b[i];
    }
    n
}

/// xor_bytes xors the bytes in a and b. The destination is assumed to have enough
/// space. Returns the number of bytes xor'd.
pub fn xor_bytes(dst: &mut [u8], a: &[u8], b: &[u8]) -> usize {
    //TODO: if supportsUnaligned {
    //	return fastXORBytes(dst, a, b)
    //}
    safe_xor_bytes(dst, a, b)
}

/// XORMappedAddress implements XOR-MAPPED-ADDRESS attribute.
///
/// RFC 5389 Section 15.2
pub struct XorMappedAddress {
    pub ip: IpAddr,
    pub port: u16,
}

impl Default for XorMappedAddress {
    fn default() -> Self {
        XorMappedAddress {
            ip: IpAddr::V4(Ipv4Addr::from(0)),
            port: 0,
        }
    }
}

impl fmt::Display for XorMappedAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let family = match self.ip {
            IpAddr::V4(_) => FAMILY_IPV4,
            IpAddr::V6(_) => FAMILY_IPV6,
        };
        if family == FAMILY_IPV4 {
            write!(f, "{}:{}", self.ip, self.port)
        } else {
            write!(f, "[{}]:{}", self.ip, self.port)
        }
    }
}

impl Setter for XorMappedAddress {
    /// add_to adds XOR-MAPPED-ADDRESS to m. Can return ErrBadIPLength
    /// if len(a.IP) is invalid.
    fn add_to(&self, m: &mut Message) -> Result<()> {
        self.add_to_as(m, ATTR_XORMAPPED_ADDRESS)
    }
}

impl Getter for XorMappedAddress {
    /// get_from decodes XOR-MAPPED-ADDRESS attribute in message and returns
    /// error if any. While decoding, a.IP is reused if possible and can be
    /// rendered to invalid state (e.g. if a.IP was set to IPv6 and then
    /// IPv4 value were decoded into it), be careful.
    fn get_from(&mut self, m: &Message) -> Result<()> {
        self.get_from_as(m, ATTR_XORMAPPED_ADDRESS)
    }
}

impl XorMappedAddress {
    /// add_to_as adds XOR-MAPPED-ADDRESS value to m as t attribute.
    pub fn add_to_as(&self, m: &mut Message, t: AttrType) -> Result<()> {
        let (family, ip_len, ip) = match self.ip {
            IpAddr::V4(ipv4) => (FAMILY_IPV4, IPV4LEN, ipv4.octets().to_vec()),
            IpAddr::V6(ipv6) => (FAMILY_IPV6, IPV6LEN, ipv6.octets().to_vec()),
        };

        let mut value = vec![0; 32 + 128];
        //value[0] = 0 // first 8 bits are zeroes
        let mut xor_value = vec![0; IPV6LEN];
        xor_value[4..].copy_from_slice(&m.transaction_id.0);
        xor_value[0..4].copy_from_slice(&MAGIC_COOKIE.to_be_bytes());
        value[0..2].copy_from_slice(&family.to_be_bytes());
        value[2..4].copy_from_slice(&(self.port ^ (MAGIC_COOKIE >> 16) as u16).to_be_bytes());
        xor_bytes(&mut value[4..4 + ip_len], &ip, &xor_value);
        m.add(t, &value[..4 + ip_len]);
        Ok(())
    }

    /// get_from_as decodes XOR-MAPPED-ADDRESS attribute value in message
    /// getting it as for t type.
    pub fn get_from_as(&mut self, m: &Message, t: AttrType) -> Result<()> {
        let v = m.get(t)?;
        if v.len() <= 4 {
            return Err(Error::ErrUnexpectedEof);
        }

        let family = u16::from_be_bytes([v[0], v[1]]);
        if family != FAMILY_IPV6 && family != FAMILY_IPV4 {
            return Err(Error::Other(format!("bad value {family}")));
        }

        check_overflow(
            t,
            v[4..].len(),
            if family == FAMILY_IPV4 {
                IPV4LEN
            } else {
                IPV6LEN
            },
        )?;
        self.port = u16::from_be_bytes([v[2], v[3]]) ^ (MAGIC_COOKIE >> 16) as u16;
        let mut xor_value = vec![0; 4 + TRANSACTION_ID_SIZE];
        xor_value[0..4].copy_from_slice(&MAGIC_COOKIE.to_be_bytes());
        xor_value[4..].copy_from_slice(&m.transaction_id.0);

        if family == FAMILY_IPV6 {
            let mut ip = [0; IPV6LEN];
            xor_bytes(&mut ip, &v[4..], &xor_value);
            self.ip = IpAddr::V6(Ipv6Addr::from(ip));
        } else {
            let mut ip = [0; IPV4LEN];
            xor_bytes(&mut ip, &v[4..], &xor_value);
            self.ip = IpAddr::V4(Ipv4Addr::from(ip));
        };

        Ok(())
    }
}
