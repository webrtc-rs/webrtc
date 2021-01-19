#[cfg(test)]
mod periodic_timer_test;

use tokio::sync::{mpsc, Mutex};
use tokio::time::Duration;

use std::sync::Arc;

use async_trait::async_trait;

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum TimerIdRefresh {
    Alloc,
    Perms,
}

impl Default for TimerIdRefresh {
    fn default() -> Self {
        TimerIdRefresh::Alloc
    }
}

// PeriodicTimerTimeoutHandler is a handler called on timeout
#[async_trait]
pub trait PeriodicTimerTimeoutHandler {
    async fn on_timeout(&mut self, id: TimerIdRefresh);
}

// PeriodicTimer is a periodic timer
#[derive(Default)]
pub struct PeriodicTimer {
    id: TimerIdRefresh,
    interval: Duration,
    close_tx: Option<mpsc::Sender<()>>,
}

impl PeriodicTimer {
    // create a new timer
    pub fn new(id: TimerIdRefresh, interval: Duration) -> Self {
        PeriodicTimer {
            id,
            interval,
            close_tx: None,
        }
    }

    // Start starts the timer.
    pub fn start<T: 'static + PeriodicTimerTimeoutHandler + std::marker::Send>(
        &mut self,
        timeout_handler: Arc<Mutex<T>>,
    ) -> bool {
        // this is a noop if the timer is always running
        if self.close_tx.is_some() {
            return false;
        }

        let (close_tx, mut close_rx) = mpsc::channel(1);
        let interval = self.interval;
        let id = self.id;

        tokio::spawn(async move {
            loop {
                let timer = tokio::time::sleep(interval);
                tokio::pin!(timer);

                tokio::select! {
                    _ = timer.as_mut() => {
                        let mut handler = timeout_handler.lock().await;
                        handler.on_timeout(id).await;
                    }
                    _ = close_rx.recv() => break,
                }
            }
        });

        self.close_tx = Some(close_tx);
        true
    }

    // Stop stops the timer.
    pub fn stop(&mut self) {
        self.close_tx.take();
    }

    // is_running tests if the timer is running.
    // Debug purpose only
    pub fn is_running(&self) -> bool {
        self.close_tx.is_some()
    }
}
