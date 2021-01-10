#[cfg(test)]
mod periodic_timer_test;

use tokio::sync::mpsc;
use tokio::time::Duration;

// PeriodicTimerTimeoutHandler is a handler called on timeout
pub type PeriodicTimerTimeoutHandler = fn(usize);

// PeriodicTimer is a periodic timer
#[derive(Debug, Default)]
pub struct PeriodicTimer {
    id: usize,
    interval: Duration,
    timeout_handler: Option<PeriodicTimerTimeoutHandler>,
    close_tx: Option<mpsc::Sender<()>>,
    //mutex          :sync.RWMutex
}

impl PeriodicTimer {
    // create a new timer
    pub fn new(
        id: usize,
        timeout_handler: Option<PeriodicTimerTimeoutHandler>,
        interval: Duration,
    ) -> Self {
        PeriodicTimer {
            id,
            interval,
            timeout_handler,
            close_tx: None,
        }
    }

    // Start starts the timer.
    pub fn start(&mut self) -> bool {
        // this is a noop if the timer is always running
        if self.close_tx.is_some() {
            return false;
        }

        let (close_tx, mut close_rx) = mpsc::channel(1);
        let interval = self.interval;
        let id = self.id;
        let timeout_handler = self.timeout_handler;

        tokio::spawn(async move {
            loop {
                let timer = tokio::time::sleep(interval);
                tokio::pin!(timer);

                tokio::select! {
                    _ = timer.as_mut() => {
                        if let Some(handler) = timeout_handler{
                            handler(id);
                        }
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
