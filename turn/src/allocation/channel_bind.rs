#[cfg(test)]
mod channel_bind_test;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use portable_atomic::AtomicBool;
use tokio::sync::Mutex;
use tokio::time::{Duration, Instant};

use super::*;
use crate::proto::channum::*;

/// `ChannelBind` represents a TURN Channel.
///
/// https://tools.ietf.org/html/rfc5766#section-2.5.
#[derive(Clone)]
pub struct ChannelBind {
    pub(crate) peer: SocketAddr,
    pub(crate) number: ChannelNumber,
    pub(crate) channel_bindings: Option<Arc<Mutex<HashMap<ChannelNumber, ChannelBind>>>>,
    reset_tx: Option<mpsc::Sender<Duration>>,
    timer_expired: Arc<AtomicBool>,
}

impl ChannelBind {
    /// Creates a new [`ChannelBind`]
    pub fn new(number: ChannelNumber, peer: SocketAddr) -> Self {
        ChannelBind {
            number,
            peer,
            channel_bindings: None,
            reset_tx: None,
            timer_expired: Arc::new(AtomicBool::new(false)),
        }
    }

    pub(crate) async fn start(&mut self, lifetime: Duration) {
        let (reset_tx, mut reset_rx) = mpsc::channel(1);
        self.reset_tx = Some(reset_tx);

        let channel_bindings = self.channel_bindings.clone();
        let number = self.number;
        let timer_expired = Arc::clone(&self.timer_expired);

        tokio::spawn(async move {
            let timer = tokio::time::sleep(lifetime);
            tokio::pin!(timer);
            let mut done = false;

            while !done {
                tokio::select! {
                    _ = &mut timer => {
                        if let Some(cbs) = &channel_bindings{
                            let mut cb = cbs.lock().await;
                            if cb.remove(&number).is_none() {
                                log::error!("Failed to remove ChannelBind for {}", number);
                            }
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
