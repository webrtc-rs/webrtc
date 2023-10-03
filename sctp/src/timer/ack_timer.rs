use std::sync::Weak;

use async_trait::async_trait;
use tokio::sync::{mpsc, Mutex};
use tokio::time::Duration;

pub(crate) const ACK_INTERVAL: Duration = Duration::from_millis(200);

/// ackTimerObserver is the interface to an ack timer observer.
#[async_trait]
pub(crate) trait AckTimerObserver {
    async fn on_ack_timeout(&mut self);
}

/// ackTimer provides the retnransmission timer conforms with RFC 4960 Sec 6.3.1
#[derive(Default, Debug)]
pub(crate) struct AckTimer<T: 'static + AckTimerObserver + Send> {
    pub(crate) timeout_observer: Weak<Mutex<T>>,
    pub(crate) interval: Duration,
    pub(crate) close_tx: Option<mpsc::Sender<()>>,
}

impl<T: 'static + AckTimerObserver + Send> AckTimer<T> {
    /// newAckTimer creates a new acknowledgement timer used to enable delayed ack.
    pub(crate) fn new(timeout_observer: Weak<Mutex<T>>, interval: Duration) -> Self {
        AckTimer {
            timeout_observer,
            interval,
            close_tx: None,
        }
    }

    /// start starts the timer.
    pub(crate) fn start(&mut self) -> bool {
        // this timer is already closed
        if self.close_tx.is_some() {
            return false;
        }

        let (close_tx, mut close_rx) = mpsc::channel(1);
        let interval = self.interval;
        let timeout_observer = self.timeout_observer.clone();

        tokio::spawn(async move {
            let timer = tokio::time::sleep(interval);
            tokio::pin!(timer);

            tokio::select! {
                _ = timer.as_mut() => {
                    if let Some(observer) = timeout_observer.upgrade(){
                        let mut observer = observer.lock().await;
                        observer.on_ack_timeout().await;
                    }
                 }
                _ = close_rx.recv() => {},
            }
        });

        self.close_tx = Some(close_tx);
        true
    }

    /// stops the timer. this is similar to stop() but subsequent start() call
    /// will fail (the timer is no longer usable)
    pub(crate) fn stop(&mut self) {
        self.close_tx.take();
    }

    /// isRunning tests if the timer is running.
    /// Debug purpose only
    pub(crate) fn is_running(&self) -> bool {
        self.close_tx.is_some()
    }
}
