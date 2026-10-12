//! Server-wide, single-use reservations of actual odd-port relay sockets.

use std::collections::{hash_map::Entry, HashMap};
use std::net::SocketAddr;
use std::sync::Arc;

use tokio::sync::Mutex;
use tokio::time::{Duration, Instant};
use util::Conn;

pub(crate) type Relay = (Arc<dyn Conn + Send + Sync>, SocketAddr);

const RESERVATION_LIFETIME: Duration = Duration::from_secs(30);

struct Reservation {
    relay: Relay,
    expires_at: Instant,
}

#[derive(Default)]
pub(crate) struct Reservations {
    entries: Mutex<HashMap<Vec<u8>, Reservation>>,
}

impl Reservations {
    pub(crate) async fn insert(self: &Arc<Self>, relay: Relay) -> Vec<u8> {
        let expires_at = Instant::now() + RESERVATION_LIFETIME;
        let token = {
            let mut entries = self.entries.lock().await;
            loop {
                let token = rand::random::<[u8; 8]>().to_vec();
                if let Entry::Vacant(entry) = entries.entry(token.clone()) {
                    entry.insert(Reservation { relay, expires_at });
                    break token;
                }
            }
        };
        // A timer must not keep a closed server and its sockets alive.
        let reservations = Arc::downgrade(self);
        let expiring = token.clone();
        tokio::spawn(async move {
            tokio::time::sleep_until(expires_at).await;
            if let Some(reservations) = reservations.upgrade() {
                let expired = {
                    let mut entries = reservations.entries.lock().await;
                    // An old timer must not remove a later reservation if a
                    // consumed token happens to be generated again.
                    if entries
                        .get(&expiring)
                        .is_some_and(|r| r.expires_at == expires_at)
                    {
                        entries.remove(&expiring)
                    } else {
                        None
                    }
                };
                if let Some(reservation) = expired {
                    let _ = reservation.relay.0.close().await;
                }
            }
        });
        token
    }

    pub(crate) async fn take(&self, token: &[u8]) -> Option<Relay> {
        let reservation = self.entries.lock().await.remove(token)?;
        // Enforce the deadline even if the expiry task has not been scheduled.
        if Instant::now() >= reservation.expires_at {
            let _ = reservation.relay.0.close().await;
            None
        } else {
            Some(reservation.relay)
        }
    }

    pub(crate) async fn clear(&self) {
        let entries = std::mem::take(&mut *self.entries.lock().await);
        for reservation in entries.into_values() {
            let _ = reservation.relay.0.close().await;
        }
    }
}
