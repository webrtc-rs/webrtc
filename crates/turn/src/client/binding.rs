#[cfg(test)]
mod binding_test;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Instant;

//  Chanel number:
//    0x4000 through 0x7FFF: These values are the allowed channel
//    numbers (16,383 possible values).
const MIN_CHANNEL_NUMBER: u16 = 0x4000;
const MAX_CHANNEL_NUMBER: u16 = 0x7fff;

#[derive(Copy, Clone, Debug, PartialEq)]
enum BindingState {
    Idle,
    Request,
    Ready,
    Refresh,
    Failed,
}

#[derive(Copy, Clone, Debug, PartialEq)]
struct Binding {
    number: u16,      // read-only
    st: BindingState, // thread-safe (atomic op)
    addr: SocketAddr, // read-only
    //TODO: mgr: BindingManager, // read-only
    //muBind       :sync.Mutex      // thread-safe, for ChannelBind ops
    refreshed_at: Instant, // protected by mutex
                           //mutex        :sync.RWMutex    // thread-safe
}

impl Binding {
    fn set_tate(&mut self, state: BindingState) {
        //atomic.StoreInt32((*int32)(&b.st), int32(state))
        self.st = state;
    }

    fn state(&self) -> BindingState {
        //return BindingState(atomic.LoadInt32((*int32)(&b.st)))
        self.st
    }

    fn set_refreshed_at(&mut self, at: Instant) {
        self.refreshed_at = at;
    }

    fn refreshed_at(&self) -> Instant {
        self.refreshed_at
    }
}
// Thread-safe Binding map
#[derive(Default)]
struct BindingManager {
    chan_map: HashMap<u16, Binding>,
    addr_map: HashMap<String, Binding>,
    next: u16,
    //mutex   :sync.RWMutex,
}

impl BindingManager {
    fn new() -> Self {
        BindingManager {
            chan_map: HashMap::new(),
            addr_map: HashMap::new(),
            next: MIN_CHANNEL_NUMBER,
        }
    }

    fn assign_channel_number(&mut self) -> u16 {
        let n = self.next;
        if self.next == MAX_CHANNEL_NUMBER {
            self.next = MIN_CHANNEL_NUMBER;
        } else {
            self.next += 1;
        }
        n
    }

    fn create(&mut self, addr: SocketAddr) -> Binding {
        let b = Binding {
            number: self.assign_channel_number(),
            st: BindingState::Idle,
            addr,
            //TODO: mgr:          mgr,
            refreshed_at: Instant::now(),
        };

        self.chan_map.insert(b.number, b);
        self.addr_map.insert(b.addr.to_string(), b);
        b
    }

    fn find_by_addr(&self, addr: SocketAddr) -> Option<&Binding> {
        self.addr_map.get(&addr.to_string())
    }

    fn find_by_number(&self, number: u16) -> Option<&Binding> {
        self.chan_map.get(&number)
    }

    fn delete_by_addr(&mut self, addr: SocketAddr) -> bool {
        if let Some(b) = self.addr_map.remove(&addr.to_string()) {
            self.chan_map.remove(&b.number);
            true
        } else {
            false
        }
    }

    fn delete_by_number(&mut self, number: u16) -> bool {
        if let Some(b) = self.chan_map.remove(&number) {
            self.addr_map.remove(&b.addr.to_string());
            true
        } else {
            false
        }
    }

    fn size(&self) -> usize {
        self.chan_map.len()
    }
}
