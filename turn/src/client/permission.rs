use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use portable_atomic::AtomicU8;

#[derive(Default, Copy, Clone, PartialEq, Debug)]
pub(crate) enum PermState {
    #[default]
    Idle = 0,
    Permitted = 1,
}

impl From<u8> for PermState {
    fn from(v: u8) -> Self {
        match v {
            0 => PermState::Idle,
            _ => PermState::Permitted,
        }
    }
}

#[derive(Default)]
pub(crate) struct Permission {
    st: AtomicU8, //PermState,
}

impl Permission {
    pub(crate) fn set_state(&self, state: PermState) {
        self.st.store(state as u8, Ordering::SeqCst);
    }

    pub(crate) fn state(&self) -> PermState {
        self.st.load(Ordering::SeqCst).into()
    }
}

/// Thread-safe Permission map.
#[derive(Default)]
pub(crate) struct PermissionMap {
    perm_map: HashMap<String, Arc<Permission>>,
}

impl PermissionMap {
    pub(crate) fn new() -> PermissionMap {
        PermissionMap {
            perm_map: HashMap::new(),
        }
    }

    pub(crate) fn insert(&mut self, addr: &SocketAddr, p: Arc<Permission>) {
        self.perm_map.insert(addr.ip().to_string(), p);
    }

    pub(crate) fn find(&self, addr: &SocketAddr) -> Option<&Arc<Permission>> {
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
