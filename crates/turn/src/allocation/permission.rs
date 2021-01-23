use super::*;

use tokio::sync::Mutex;
use tokio::time::{Duration, Instant};

use std::sync::Arc;

pub(crate) const PERMISSION_TIMEOUT: Duration = Duration::from_secs(5 * 60);

// Permission represents a TURN permission. TURN permissions mimic the address-restricted
// filtering mechanism of NATs that comply with [RFC4787].
// https://tools.ietf.org/html/rfc5766#section-2.3
pub struct Permission {
    pub(crate) addr: SocketAddr,
    pub(crate) permissions: Option<Arc<Mutex<HashMap<String, Permission>>>>,
    reset_tx: Option<mpsc::Sender<Duration>>,
}

impl Permission {
    // NewPermission create a new Permission
    pub fn new(addr: SocketAddr) -> Self {
        Permission {
            addr,
            permissions: None,
            reset_tx: None,
        }
    }

    pub(crate) async fn start(&mut self, lifetime: Duration) {
        let (reset_tx, mut reset_rx) = mpsc::channel(1);
        self.reset_tx = Some(reset_tx);

        let permissions = self.permissions.clone();
        let addr = self.addr;

        tokio::spawn(async move {
            let timer = tokio::time::sleep(lifetime);
            tokio::pin!(timer);

            loop {
                tokio::select! {
                    _ = &mut timer => {
                        if let Some(permissions) = permissions{
                            let mut permissions = permissions.lock().await;
                            permissions.remove(&addr2ipfingerprint(&addr));
                        }
                        break;
                    },
                    result = reset_rx.recv() => {
                        if let Some(d) = result {
                            timer.as_mut().reset(Instant::now() + d);
                        }
                    },
                }
            }
        });
    }

    pub(crate) async fn refresh(&self, lifetime: Duration) {
        if let Some(tx) = &self.reset_tx {
            let _ = tx.send(lifetime).await;
        }
    }
}
