use std::net::SocketAddr;

pub struct Interface {
    ifc: ifaces::Interface,
    addrs: Vec<SocketAddr>,
}

impl Interface {
    pub fn new(ifc: ifaces::Interface) -> Self {
        Interface { ifc, addrs: vec![] }
    }

    pub fn add(&mut self, addr: SocketAddr) {
        self.addrs.push(addr);
    }

    pub fn addrs(&self) -> &[SocketAddr] {
        &self.addrs
    }
}
