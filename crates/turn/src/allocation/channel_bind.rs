#[cfg(test)]
mod channel_bind_test;

use super::*;
use crate::proto::channum::*;

use tokio::sync::Mutex;
use tokio::time::{Duration, Instant};

use std::sync::Arc;

// ChannelBind represents a TURN Channel
// https://tools.ietf.org/html/rfc5766#section-2.5
#[derive(Clone)]
pub struct ChannelBind {
    pub(crate) peer: SocketAddr,
    pub(crate) number: ChannelNumber,
    pub(crate) channel_bindings: Option<Arc<Mutex<HashMap<ChannelNumber, ChannelBind>>>>,
    reset_tx: Option<mpsc::Sender<Duration>>,
}

impl ChannelBind {
    // NewChannelBind creates a new ChannelBind
    pub fn new(number: ChannelNumber, peer: SocketAddr) -> Self {
        ChannelBind {
            number,
            peer,
            channel_bindings: None,
            reset_tx: None,
        }
    }

    pub(crate) async fn start(&mut self, lifetime: Duration) {
        let (reset_tx, mut reset_rx) = mpsc::channel(1);
        self.reset_tx = Some(reset_tx);

        let channel_bindings = self.channel_bindings.clone();
        let number = self.number;

        tokio::spawn(async move {
            let timer = tokio::time::sleep(lifetime);
            tokio::pin!(timer);

            loop {
                tokio::select! {
                    _ = &mut timer => {
                        if let Some(channel_bindings) = channel_bindings{
                            let mut channel_bindings = channel_bindings.lock().await;
                            if channel_bindings.remove(&number).is_none() {
                                log::error!("Failed to remove ChannelBind for {}", number);
                            }
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
