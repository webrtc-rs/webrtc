#[cfg(test)]
mod operation_test;

use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use portable_atomic::AtomicUsize;
use tokio::sync::mpsc;
use waitgroup::WaitGroup;

use crate::error::Result;

/// Operation is a function
pub struct Operation(
    pub Box<dyn (FnMut() -> Pin<Box<dyn Future<Output = bool> + Send + 'static>>) + Send + Sync>,
    pub &'static str,
);

impl Operation {
    pub(crate) fn new(
        op: impl FnMut() -> Pin<Box<dyn Future<Output = bool> + Send + 'static>> + Send + Sync + 'static,
        description: &'static str,
    ) -> Self {
        Self(Box::new(op), description)
    }
}

impl fmt::Debug for Operation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Operation")
            .field(&"_")
            .field(&self.1)
            .finish()
    }
}

/// Operations is a task executor.
#[derive(Default)]
pub(crate) struct Operations {
    length: Arc<AtomicUsize>,
    ops_tx: Option<Arc<mpsc::UnboundedSender<Operation>>>,
    close_tx: Option<mpsc::Sender<()>>,
}

impl Operations {
    pub(crate) fn new() -> Self {
        let length = Arc::new(AtomicUsize::new(0));
        let (ops_tx, ops_rx) = mpsc::unbounded_channel();
        let (close_tx, close_rx) = mpsc::channel(1);
        let l = Arc::clone(&length);
        let ops_tx = Arc::new(ops_tx);
        let ops_tx2 = Arc::clone(&ops_tx);
        tokio::spawn(async move {
            Operations::start(l, ops_tx, ops_rx, close_rx).await;
        });

        Operations {
            length,
            ops_tx: Some(ops_tx2),
            close_tx: Some(close_tx),
        }
    }

    /// enqueue adds a new action to be executed. If there are no actions scheduled,
    /// the execution will start immediately in a new goroutine.
    pub(crate) async fn enqueue(&self, op: Operation) -> Result<()> {
        if let Some(ops_tx) = &self.ops_tx {
            return Operations::enqueue_inner(op, ops_tx, &self.length);
        }

        Ok(())
    }

    fn enqueue_inner(
        op: Operation,
        ops_tx: &Arc<mpsc::UnboundedSender<Operation>>,
        length: &Arc<AtomicUsize>,
    ) -> Result<()> {
        length.fetch_add(1, Ordering::SeqCst);
        ops_tx.send(op)?;

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
            .enqueue(Operation::new(
                move || {
                    let _d = w.take();
                    Box::pin(async { false })
                },
                "Operation::done",
            ))
            .await;
        wg.wait().await;
    }

    pub(crate) async fn start(
        length: Arc<AtomicUsize>,
        ops_tx: Arc<mpsc::UnboundedSender<Operation>>,
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
                        if f.0().await {
                            // Requeue this operation
                            let _ = Operations::enqueue_inner(f, &ops_tx, &length);
                        }
                    }
                }
            }
        }
    }

    pub(crate) async fn close(&self) -> Result<()> {
        if let Some(close_tx) = &self.close_tx {
            close_tx.send(()).await?;
        }
        Ok(())
    }
}
