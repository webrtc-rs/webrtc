use super::net::*;
use crate::Error;

use std::fmt;
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

lazy_static! {
    static ref TAG_CTR: AtomicU64 = AtomicU64::new(0);
}

/// Encodes a u64 value to a lowercase base 36 string.
pub fn base36(value: impl Into<u64>) -> String {
    let mut digits: Vec<u8> = vec![];

    let mut value = value.into();
    while value > 0 {
        let digit = (value % 36) as usize;
        value /= 36;

        digits.push(b"0123456789abcdefghijklmnopqrstuvwxyz"[digit]);
    }

    digits.reverse();
    format!("{:0>8}", String::from_utf8(digits).unwrap())
}

// Generate a base36-encoded unique tag
// See: https://play.golang.org/p/0ZaAID1q-HN
fn assign_chunk_tag() -> String {
    let n = TAG_CTR.fetch_add(1, Ordering::SeqCst);
    base36(n)
}

#[derive(Copy, Clone)]
pub(crate) enum TcpFlag {
    FIN = 0x01,
    SYN = 0x02,
    RST = 0x04,
    PSH = 0x08,
    ACK = 0x10,
}

impl fmt::Display for TcpFlag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut sa = vec![];

        let flag = *self as u8;
        if flag & TcpFlag::FIN as u8 != 0 {
            sa.push("FIN");
        }
        if flag & TcpFlag::SYN as u8 != 0 {
            sa.push("SYN");
        }
        if flag & TcpFlag::RST as u8 != 0 {
            sa.push("RST");
        }
        if flag & TcpFlag::PSH as u8 != 0 {
            sa.push("PSH");
        }
        if flag & TcpFlag::ACK as u8 != 0 {
            sa.push("ACK");
        }

        write!(f, "{}", sa.join("-"))
    }
}

// Chunk represents a packet passed around in the vnet
pub trait Chunk: fmt::Display {
    fn set_timestamp(&mut self) -> SystemTime; // used by router
    fn get_timestamp(&self) -> SystemTime; // used by router
    fn get_source_ip(&self) -> IpAddr; // used by routee
    fn get_destination_ip(&self) -> IpAddr; // used by router
    fn set_source_addr(&mut self, address: &str) -> Result<(), Error>; // used by nat
    fn set_destination_addr(&mut self, address: &str) -> Result<(), Error>; // used by nat

    fn source_addr(&self) -> SocketAddr;
    fn destination_addr(&self) -> SocketAddr;
    fn user_data(&self) -> Vec<u8>;
    fn tag(&self) -> String;
    fn clone_to(&self) -> Box<dyn Chunk>;
    fn network(&self) -> String; // returns "udp" or "tcp"
}

pub(crate) struct ChunkIP {
    pub(crate) timestamp: SystemTime,
    pub(crate) source_ip: IpAddr,
    pub(crate) destination_ip: IpAddr,
    pub(crate) tag: String,
}

impl ChunkIP {
    fn set_timestamp(&mut self) -> SystemTime {
        self.timestamp = SystemTime::now();
        self.timestamp
    }

    fn get_timestamp(&self) -> SystemTime {
        self.timestamp
    }

    fn get_destination_ip(&self) -> IpAddr {
        self.destination_ip
    }

    fn get_source_ip(&self) -> IpAddr {
        self.source_ip
    }

    fn tag(&self) -> String {
        self.tag.clone()
    }
}

pub(crate) struct ChunkUDP {
    pub(crate) chunk_ip: ChunkIP,
    pub(crate) source_port: u16,
    pub(crate) destination_port: u16,
    pub(crate) user_data: Vec<u8>,
}

impl fmt::Display for ChunkUDP {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} chunk {} {} => {}",
            self.network(),
            self.tag(),
            self.source_addr(),
            self.destination_addr(),
        )
    }
}

impl Chunk for ChunkUDP {
    fn set_timestamp(&mut self) -> SystemTime {
        self.chunk_ip.set_timestamp()
    }

    fn get_timestamp(&self) -> SystemTime {
        self.chunk_ip.get_timestamp()
    }

    fn get_destination_ip(&self) -> IpAddr {
        self.chunk_ip.get_destination_ip()
    }

    fn get_source_ip(&self) -> IpAddr {
        self.chunk_ip.get_source_ip()
    }

    fn tag(&self) -> String {
        self.chunk_ip.tag()
    }

    fn source_addr(&self) -> SocketAddr {
        SocketAddr::new(self.chunk_ip.source_ip, self.source_port)
    }

    fn destination_addr(&self) -> SocketAddr {
        SocketAddr::new(self.chunk_ip.destination_ip, self.destination_port)
    }

    fn user_data(&self) -> Vec<u8> {
        self.user_data.clone()
    }

    fn clone_to(&self) -> Box<dyn Chunk> {
        Box::new(ChunkUDP {
            chunk_ip: ChunkIP {
                timestamp: self.chunk_ip.timestamp,
                source_ip: self.chunk_ip.source_ip,
                destination_ip: self.chunk_ip.destination_ip,
                tag: self.chunk_ip.tag.clone(),
            },
            source_port: self.source_port,
            destination_port: self.destination_port,
            user_data: self.user_data.clone(),
        })
    }

    fn network(&self) -> String {
        UDP_STR.to_owned()
    }

    fn set_source_addr(&mut self, address: &str) -> Result<(), Error> {
        let addr = SocketAddr::from_str(address)?;
        self.chunk_ip.source_ip = addr.ip();
        self.source_port = addr.port();
        Ok(())
    }

    fn set_destination_addr(&mut self, address: &str) -> Result<(), Error> {
        let addr = SocketAddr::from_str(address)?;
        self.chunk_ip.destination_ip = addr.ip();
        self.destination_port = addr.port();
        Ok(())
    }
}

impl ChunkUDP {
    pub(crate) fn new(src_addr: SocketAddr, dst_addr: SocketAddr) -> Self {
        ChunkUDP {
            chunk_ip: ChunkIP {
                timestamp: SystemTime::now(),
                source_ip: src_addr.ip(),
                destination_ip: dst_addr.ip(),
                tag: assign_chunk_tag(),
            },
            source_port: src_addr.port(),
            destination_port: dst_addr.port(),
            user_data: vec![],
        }
    }
}

pub(crate) struct ChunkTCP {
    chunk_ip: ChunkIP,
    source_port: u16,
    destination_port: u16,
    flags: TcpFlag, // control bits
    user_data: Vec<u8>, // only with PSH flag
                    // seq             :u32,  // always starts with 0
                    // ack             :u32,  // always starts with 0
}

impl fmt::Display for ChunkTCP {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} chunk {} {} => {}",
            self.network(),
            self.flags,
            self.chunk_ip.tag,
            self.source_addr(),
            self.destination_addr(),
        )
    }
}

impl Chunk for ChunkTCP {
    fn set_timestamp(&mut self) -> SystemTime {
        self.chunk_ip.set_timestamp()
    }

    fn get_timestamp(&self) -> SystemTime {
        self.chunk_ip.get_timestamp()
    }

    fn get_destination_ip(&self) -> IpAddr {
        self.chunk_ip.get_destination_ip()
    }

    fn get_source_ip(&self) -> IpAddr {
        self.chunk_ip.get_source_ip()
    }

    fn tag(&self) -> String {
        self.chunk_ip.tag()
    }

    fn source_addr(&self) -> SocketAddr {
        SocketAddr::new(self.chunk_ip.source_ip, self.source_port)
    }

    fn destination_addr(&self) -> SocketAddr {
        SocketAddr::new(self.chunk_ip.destination_ip, self.destination_port)
    }

    fn user_data(&self) -> Vec<u8> {
        self.user_data.clone()
    }

    fn clone_to(&self) -> Box<dyn Chunk> {
        Box::new(ChunkTCP {
            chunk_ip: ChunkIP {
                timestamp: self.chunk_ip.timestamp,
                source_ip: self.chunk_ip.source_ip,
                destination_ip: self.chunk_ip.destination_ip,
                tag: self.chunk_ip.tag.clone(),
            },
            source_port: self.source_port,
            destination_port: self.destination_port,
            flags: self.flags,
            user_data: self.user_data.clone(),
        })
    }

    fn network(&self) -> String {
        "tcp".to_owned()
    }

    fn set_source_addr(&mut self, address: &str) -> Result<(), Error> {
        let addr = SocketAddr::from_str(address)?;
        self.chunk_ip.source_ip = addr.ip();
        self.source_port = addr.port();
        Ok(())
    }

    fn set_destination_addr(&mut self, address: &str) -> Result<(), Error> {
        let addr = SocketAddr::from_str(address)?;
        self.chunk_ip.destination_ip = addr.ip();
        self.destination_port = addr.port();
        Ok(())
    }
}

impl ChunkTCP {
    pub(crate) fn new(src_addr: SocketAddr, dst_addr: SocketAddr, flags: TcpFlag) -> Self {
        ChunkTCP {
            chunk_ip: ChunkIP {
                timestamp: SystemTime::now(),
                source_ip: src_addr.ip(),
                destination_ip: dst_addr.ip(),
                tag: assign_chunk_tag(),
            },
            source_port: src_addr.port(),
            destination_port: dst_addr.port(),
            flags,
            user_data: vec![],
        }
    }
}
