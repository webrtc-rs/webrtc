use crate::errors::*;
use crate::message::*;

use util::Error;

use std::collections::HashMap;

use tokio::sync::Mutex;
use tokio::time;

// noop_handler just discards any event.
pub fn noop_handler() -> Handler {
    Box::new(|_e| {})
}

struct AgentInternal {
    // transactions is map of transactions that are currently
    // in progress. Event handling is done in such way when
    // transaction is unregistered before AgentTransaction access,
    // minimizing mux lock and protecting AgentTransaction from
    // data races via unexpected concurrent access.
    transactions: HashMap<TransactionId, AgentTransaction>,
    closed: bool,     // all calls are invalid if true
    handler: Handler, // handles transactions
}

// Agent is low-level abstraction over transaction list that
// handles concurrency (all calls are goroutine-safe) and
// time outs (via Collect call).
pub struct Agent {
    mux: Mutex<AgentInternal>,
}

// Handler handles state changes of transaction.
//
// Handler is called on transaction state change.
// Usage of e is valid only during call, user must
// copy needed fields explicitly.
pub type Handler = Box<dyn Fn(&Event)>;

// Event is passed to Handler describing the transaction event.
// Do not reuse outside Handler.
#[derive(Default)]
pub struct Event {
    pub transaction_id: [u8; TRANSACTION_ID_SIZE],
    pub message: Message,
    pub error: Error,
}

// AgentTransaction represents transaction in progress.
// Concurrent access is invalid.
pub(crate) struct AgentTransaction {
    id: TransactionId,
    deadline: time::Duration,
}

// AGENT_COLLECT_CAP is initial capacity for Agent.Collect slices,
// sufficient to make function zero-alloc in most cases.
const AGENT_COLLECT_CAP: usize = 100;

type TransactionId = [u8; TRANSACTION_ID_SIZE];

// NewAgent initializes and returns new Agent with provided handler.
// If h is nil, the noop_handler will be used.
impl Agent {
    pub fn new(handler: Handler) -> Agent {
        let ai = AgentInternal {
            transactions: HashMap::new(),
            closed: false,
            handler,
        };
        Agent {
            mux: Mutex::new(ai),
        }
    }

    // stop_with_error removes transaction from list and calls handler with
    // provided error. Can return ErrTransactionNotExists and ErrAgentClosed.
    pub async fn stop_with_error(
        &self,
        id: [u8; TRANSACTION_ID_SIZE],
        error: Error,
    ) -> Result<(), Error> {
        let mut a = self.mux.lock().await;
        if a.closed {
            return Err(ERR_AGENT_CLOSED.clone());
        }

        let v = a.transactions.remove(&id);
        if let Some(t) = v {
            (a.handler)(&Event {
                transaction_id: t.id,
                message: Message::default(),
                error,
            });
            Ok(())
        } else {
            Err(ERR_TRANSACTION_NOT_EXISTS.clone())
        }
    }

    // Stop stops transaction by id with ErrTransactionStopped, blocking
    // until handler returns.
    pub async fn stop(&mut self, id: [u8; TRANSACTION_ID_SIZE]) -> Result<(), Error> {
        self.stop_with_error(id, ERR_TRANSACTION_STOPPED.clone())
            .await
    }

    // Start registers transaction with provided id and deadline.
    // Could return ErrAgentClosed, ErrTransactionExists.
    //
    // Agent handler is guaranteed to be eventually called.
    pub async fn start(
        &self,
        id: [u8; TRANSACTION_ID_SIZE],
        deadline: time::Duration,
    ) -> Result<(), Error> {
        let mut a = self.mux.lock().await;
        if a.closed {
            return Err(ERR_AGENT_CLOSED.clone());
        }
        if a.transactions.contains_key(&id) {
            return Err(ERR_TRANSACTION_EXISTS.clone());
        }

        a.transactions.insert(id, AgentTransaction { id, deadline });

        Ok(())
    }

    // Collect terminates all transactions that have deadline before provided
    // time, blocking until all handlers will process ErrTransactionTimeOut.
    // Will return ErrAgentClosed if agent is already closed.
    //
    // It is safe to call Collect concurrently but makes no sense.
    pub async fn collect(&self, gc_time: time::Duration) -> Result<(), Error> {
        let mut a = self.mux.lock().await;
        if a.closed {
            // Doing nothing if agent is closed.
            // All transactions should be already closed
            // during Close() call.
            return Err(ERR_AGENT_CLOSED.clone());
        }

        let mut to_remove: Vec<TransactionId> = Vec::with_capacity(AGENT_COLLECT_CAP);

        // Adding all transactions with deadline before gc_time
        // to toCall and to_remove slices.
        // No allocs if there are less than AGENT_COLLECT_CAP
        // timed out transactions.
        for (id, t) in &a.transactions {
            if t.deadline < gc_time {
                to_remove.push(*id);
            }
        }
        // Un-registering timed out transactions.
        for id in &to_remove {
            a.transactions.remove(id);
        }
        // Calling handler does not require locked mutex,
        // reducing lock time.
        //let h = a.handler.clone();
        //a.mux.Unlock()
        // Sending ErrTransactionTimeOut to handler for all transactions,
        // blocking until last one.
        let mut event = Event {
            error: ERR_TRANSACTION_TIME_OUT.clone(),
            ..Default::default()
        };
        for id in to_remove {
            event.transaction_id = id;
            (a.handler)(&event);
        }

        Ok(())
    }

    // process incoming message, synchronously passing it to handler.
    pub async fn process(&self, m: Message) -> Result<(), Error> {
        let mut a = self.mux.lock().await;
        if a.closed {
            return Err(ERR_AGENT_CLOSED.clone());
        }

        let e = Event {
            transaction_id: m.transaction_id,
            message: m,
            ..Default::default()
        };

        //h := a.handler
        a.transactions.remove(&e.transaction_id);

        (a.handler)(&e);

        Ok(())
    }

    // set_handler sets agent handler to h.
    pub async fn set_handler(&self, h: Handler) -> Result<(), Error> {
        let mut a = self.mux.lock().await;
        if a.closed {
            return Err(ERR_AGENT_CLOSED.clone());
        }
        a.handler = h;

        Ok(())
    }

    // Close terminates all transactions with ErrAgentClosed and renders Agent to
    // closed state.
    pub async fn close(&self) -> Result<(), Error> {
        let mut a = self.mux.lock().await;
        if a.closed {
            return Err(ERR_AGENT_CLOSED.clone());
        }

        let mut e = Event {
            error: ERR_AGENT_CLOSED.clone(),
            ..Default::default()
        };

        for id in a.transactions.keys() {
            e.transaction_id = *id;
            (a.handler)(&e)
        }
        a.transactions = HashMap::new();
        a.closed = true;
        a.handler = noop_handler();

        Ok(())
    }
}
