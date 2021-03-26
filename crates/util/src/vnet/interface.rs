use crate::Error;
use ipnet::*;
use std::net::SocketAddr;
use std::str::FromStr;

#[derive(Debug, Clone, Default)]
pub struct Interface {
    pub(crate) name: String,
    pub(crate) addrs: Vec<IpNet>,
}

impl Interface {
    pub fn new(name: String, addrs: Vec<IpNet>) -> Self {
        Interface { name, addrs }
    }

    pub fn add_addr(&mut self, addr: IpNet) {
        self.addrs.push(addr);
    }

    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn addrs(&self) -> &[IpNet] {
        &self.addrs
    }

    pub fn convert(addr: SocketAddr, mask: Option<SocketAddr>) -> Result<IpNet, Error> {
        let prefix = if let Some(mask) = mask {
            match (addr, mask) {
                (SocketAddr::V4(_), SocketAddr::V4(mask)) => {
                    let octets = mask.ip().octets();
                    let mut prefix = 0;
                    for octet in &octets {
                        for i in 0..8 {
                            prefix += (*octet >> (7 - i)) & 0x1;
                        }
                    }
                    prefix
                }
                (SocketAddr::V6(_), SocketAddr::V6(mask)) => {
                    let octets = mask.ip().octets();
                    let mut prefix = 0;
                    for octet in &octets {
                        for i in 0..8 {
                            prefix += (*octet >> (7 - i)) & 0x1;
                        }
                    }
                    prefix
                }
                _ => return Err(Error::new("Invalid mask".to_string())),
            }
        } else {
            32
        };
        let s = format!("{}/{}", addr.ip(), prefix);

        Ok(IpNet::from_str(&s)?)
    }
}
