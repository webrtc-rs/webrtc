use std::sync::atomic::Ordering;
use std::sync::Arc;

use portable_atomic::AtomicBool;
use tokio::sync::Mutex;
use tokio::time::{Duration, Instant};

use super::*;

pub(crate) const PERMISSION_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// `Permission` represents a TURN permission. TURN permissions mimic the address-restricted
/// filtering mechanism of NATs that comply with [RFC4787].
///
/// https://tools.ietf.org/html/rfc5766#section-2.3
pub struct Permission {
    pub(crate) addr: SocketAddr,
    pub(crate) permissions: Option<Arc<Mutex<HashMap<String, Permission>>>>,
    reset_tx: Option<mpsc::Sender<Duration>>,
    timer_expired: Arc<AtomicBool>,
}

impl Permission {
    /// Creates a new [`Permission`].
    pub fn new(addr: SocketAddr) -> Self {
        Permission {
            addr,
            permissions: None,
            reset_tx: None,
            timer_expired: Arc::new(AtomicBool::new(false)),
        }
    }

    pub(crate) async fn start(&mut self, lifetime: Duration) {
        let (reset_tx, mut reset_rx) = mpsc::channel(1);
        self.reset_tx = Some(reset_tx);

        let permissions = self.permissions.clone();
        let addr = self.addr;
        let timer_expired = Arc::clone(&self.timer_expired);

        tokio::spawn(async move {
            let timer = tokio::time::sleep(lifetime);
            tokio::pin!(timer);
            let mut done = false;

            while !done {
                tokio::select! {
                    _ = &mut timer => {
                        if let Some(perms) = &permissions{
                            let mut p = perms.lock().await;
                            p.remove(&addr2ipfingerprint(&addr));
                        }
                        done = true;
                    },
                    result = reset_rx.recv() => {
                        if let Some(d) = result {
                            timer.as_mut().reset(Instant::now() + d);
                        } else {
                            done = true;
                        }
                    },
                }
            }

            timer_expired.store(true, Ordering::SeqCst);
        });
    }

    pub(crate) fn stop(&mut self) -> bool {
        let expired = self.reset_tx.is_none() || self.timer_expired.load(Ordering::SeqCst);
        self.reset_tx.take();
        expired
    }

    pub(crate) async fn refresh(&self, lifetime: Duration) {
        if let Some(tx) = &self.reset_tx {
            let _ = tx.send(lifetime).await;
        }
    }
}
