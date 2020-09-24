use tokio::sync::{mpsc, Mutex};
use tokio::time::{delay_for, Duration};

#[cfg(test)]
mod deadline_test;

// Deadline signals updatable deadline timer.
// Also, it implements context.Context.
struct DeadlineInternal {
    exceeded_tx: Option<mpsc::Sender<()>>,
    exceeded_rx: Option<mpsc::Receiver<()>>,

    stop_tx: Option<mpsc::Sender<()>>,
    stop_rx: Option<mpsc::Receiver<()>>,

    stopped_tx: Option<mpsc::Sender<bool>>,
    stopped_rx: Option<mpsc::Receiver<bool>>,

    deadline: Duration,
}

pub(crate) struct Deadline {
    mu: Mutex<DeadlineInternal>,
}

impl Deadline {
    pub(crate) async fn new() -> Self {
        let (exceeded_tx, exceeded_rx) = mpsc::channel(1);
        let (stop_tx, stop_rx) = mpsc::channel(1);
        let (stopped_tx, stopped_rx) = mpsc::channel(1);
        let deadline = Deadline {
            mu: Mutex::new(DeadlineInternal {
                exceeded_tx: Some(exceeded_tx),
                exceeded_rx: Some(exceeded_rx),

                stop_tx: Some(stop_tx),
                stop_rx: Some(stop_rx),

                stopped_tx: Some(stopped_tx),
                stopped_rx: Some(stopped_rx),

                deadline: Duration::new(0, 0),
            }),
        };

        {
            let mut d = deadline.mu.lock().await;
            if let Some(stopped_tx) = d.stopped_tx.as_mut() {
                let _ = stopped_tx.send(true).await;
            }
        }

        deadline
    }

    pub(crate) async fn set(&self, t: Duration) {
        let mut d = self.mu.lock().await;

        d.deadline = t;

        d.stop_tx.take();

        let mut exceeded_rx = d.exceeded_rx.take();

        tokio::select! {
            _ = exceeded_rx.as_mut().unwrap().recv() => {
                let (exceeded_tx, exceeded_rx) = mpsc::channel(1);
                d.exceeded_tx = Some(exceeded_tx);
                d.exceeded_rx = Some(exceeded_rx);
            }
            stopped = d.stopped_rx.as_mut().unwrap().recv() => {
                if let Some(stopped) = stopped{
                    if !stopped {
                        let (exceeded_tx, exceeded_rx) = mpsc::channel(1);
                        d.exceeded_tx = Some(exceeded_tx);
                        d.exceeded_rx = Some(exceeded_rx);
                    }
                }
            }
        };

        let (stop_tx, stop_rx) = mpsc::channel(1);
        let (stopped_tx, stopped_rx) = mpsc::channel(1);

        d.stop_tx = Some(stop_tx);
        d.stop_rx = Some(stop_rx);
        d.stopped_tx = Some(stopped_tx);
        d.stopped_rx = Some(stopped_rx);

        if t.as_nanos() == 0 {
            let _ = d.stopped_tx.as_mut().unwrap().send(true).await;
            return;
        }

        let exceeded_tx = d.exceeded_tx.take();
        let mut stopped_tx = d.stopped_tx.take();
        let mut stop_rx = d.stop_rx.take();
        tokio::spawn(async move {
            tokio::select! {
                _ = delay_for(t) => {
                    drop(exceeded_tx);
                    let _ = stopped_tx.as_mut().unwrap().send(false).await;
                }
                _ = stop_rx.as_mut().unwrap().recv() => {
                    let _ = stopped_tx.as_mut().unwrap().send(false).await;
                }
            };
        });
    }

    // Done receives deadline signal.
    pub(crate) async fn done(&self) -> Option<()> {
        let mut d = self.mu.lock().await;

        if let Some(exceeded_rx) = d.exceeded_rx.as_mut() {
            exceeded_rx.recv().await
        } else {
            Some(())
        }
    }

    pub(crate) async fn deadline(&self) -> Duration {
        let d = self.mu.lock().await;

        d.deadline
    }
}
