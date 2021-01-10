use std::collections::HashMap;
use std::net::SocketAddr;

#[derive(Copy, Clone, PartialEq, Debug)]
enum PermState {
    Idle,
    Permitted,
}

struct Permission {
    st: PermState,
}

impl Permission {
    fn set_state(&mut self, state: PermState) {
        self.st = state;
    }

    fn state(&self) -> PermState {
        self.st
    }
}

// Thread-safe Permission map
#[derive(Default)]
pub(crate) struct PermissionMap {
    perm_map: HashMap<String, Permission>,
}

impl PermissionMap {
    fn new() -> PermissionMap {
        PermissionMap {
            perm_map: HashMap::new(),
        }
    }

    fn insert(&mut self, addr: SocketAddr, p: Permission) {
        self.perm_map.insert(addr.ip().to_string(), p);
    }

    fn find(&self, addr: SocketAddr) -> Option<&Permission> {
        self.perm_map.get(&addr.ip().to_string())
    }

    fn delete(&mut self, addr: SocketAddr) {
        self.perm_map.remove(&addr.ip().to_string());
    }

    fn addrs(&self) -> Vec<SocketAddr> {
        let mut a = vec![];
        for k in self.perm_map.keys() {
            if let Ok(ip) = k.parse() {
                a.push(SocketAddr::new(ip, 0));
            }
        }
        a
    }
}
