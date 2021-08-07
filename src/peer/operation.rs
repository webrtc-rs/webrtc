use anyhow::Result;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use waitgroup::WaitGroup;

/// Operation is a function
pub struct Operation(
    pub Box<dyn (FnMut() -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>) + Send + Sync>,
);

impl fmt::Debug for Operation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Operation").finish()
    }
}

/// Operations is a task executor.
#[derive(Default)]
pub(crate) struct Operations {
    length: Arc<AtomicUsize>,
    ops_tx: Option<mpsc::UnboundedSender<Operation>>,
    close_tx: Option<mpsc::Sender<()>>,
}

impl Operations {
    pub(crate) fn new() -> Self {
        let length = Arc::new(AtomicUsize::new(0));
        let (ops_tx, ops_rx) = mpsc::unbounded_channel();
        let (close_tx, close_rx) = mpsc::channel(1);
        let l = Arc::clone(&length);
        tokio::spawn(async move {
            Operations::start(l, ops_rx, close_rx).await;
        });

        Operations {
            length,
            ops_tx: Some(ops_tx),
            close_tx: Some(close_tx),
        }
    }

    /// enqueue adds a new action to be executed. If there are no actions scheduled,
    /// the execution will start immediately in a new goroutine.
    pub(crate) async fn enqueue(&self, op: Operation) -> Result<()> {
        if let Some(ops_tx) = &self.ops_tx {
            let _ = ops_tx.send(op)?;
            self.length.fetch_add(1, Ordering::SeqCst);
        }
        Ok(())
    }

    /// is_empty checks if there are tasks in the queue
    pub(crate) async fn is_empty(&self) -> bool {
        self.length.load(Ordering::SeqCst) == 0
    }

    /// Done blocks until all currently enqueued operations are finished executing.
    /// For more complex synchronization, use Enqueue directly.
    pub(crate) async fn done(&self) {
        let wg = WaitGroup::new();
        let mut w = Some(wg.worker());
        let _ = self
            .enqueue(Operation(Box::new(move || {
                let _d = w.take();
                Box::pin(async {})
            })))
            .await;
        wg.wait().await;
    }

    pub(crate) async fn start(
        length: Arc<AtomicUsize>,
        mut ops_rx: mpsc::UnboundedReceiver<Operation>,
        mut close_rx: mpsc::Receiver<()>,
    ) {
        loop {
            tokio::select! {
                _ = close_rx.recv() => {
                    break;
                }
                result = ops_rx.recv() => {
                    if let Some(mut f) = result {
                        length.fetch_sub(1, Ordering::SeqCst);
                        f.0().await;
                    }
                }
            }
        }
    }

    pub(crate) async fn close(&self) -> Result<()> {
        if let Some(close_tx) = &self.close_tx {
            let _ = close_tx.send(()).await?;
        }
        Ok(())
    }
}
