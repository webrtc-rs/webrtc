use std::collections::HashMap;
use std::net::SocketAddr;

#[derive(Copy, Clone, PartialEq, Debug)]
pub(crate) enum PermState {
    Idle,
    Permitted,
}

impl Default for PermState {
    fn default() -> Self {
        PermState::Idle
    }
}

#[derive(Default, Copy, Clone)]
pub(crate) struct Permission {
    st: PermState,
}

impl Permission {
    pub(crate) fn set_state(&mut self, state: PermState) {
        self.st = state;
    }

    pub(crate) fn state(&self) -> PermState {
        self.st
    }
}

// Thread-safe Permission map
#[derive(Default)]
pub(crate) struct PermissionMap {
    perm_map: HashMap<String, Permission>,
}

impl PermissionMap {
    pub(crate) fn new() -> PermissionMap {
        PermissionMap {
            perm_map: HashMap::new(),
        }
    }

    pub(crate) fn insert(&mut self, addr: &SocketAddr, p: Permission) {
        self.perm_map.insert(addr.ip().to_string(), p);
    }

    pub(crate) fn find(&self, addr: &SocketAddr) -> Option<&Permission> {
        self.perm_map.get(&addr.ip().to_string())
    }

    pub(crate) fn delete(&mut self, addr: &SocketAddr) {
        self.perm_map.remove(&addr.ip().to_string());
    }

    pub(crate) fn addrs(&self) -> Vec<SocketAddr> {
        let mut a = vec![];
        for k in self.perm_map.keys() {
            if let Ok(ip) = k.parse() {
                a.push(SocketAddr::new(ip, 0));
            }
        }
        a
    }
}
