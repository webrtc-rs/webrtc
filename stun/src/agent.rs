#[cfg(test)]
mod agent_test;

use crate::errors::*;
use crate::message::*;

use util::Error;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use rand::Rng;

// Handler handles state changes of transaction.
//
// Handler is called on transaction state change.
// Usage of e is valid only during call, user must
// copy needed fields explicitly.
pub type Handler = Box<dyn Fn(&Event)>;

// noop_handler just discards any event.
pub fn noop_handler() -> Handler {
    Box::new(|_e| {})
}

// Agent is low-level abstraction over transaction list that
// handles concurrency (all calls are goroutine-safe) and
// time outs (via Collect call).
pub struct Agent {
    // transactions is map of transactions that are currently
    // in progress. Event handling is done in such way when
    // transaction is unregistered before AgentTransaction access,
    // minimizing mux lock and protecting AgentTransaction from
    // data races via unexpected concurrent access.
    transactions: HashMap<TransactionId, AgentTransaction>,
    closed: bool,     // all calls are invalid if true
    handler: Handler, // handles transactions
}

// Event is passed to Handler describing the transaction event.
// Do not reuse outside Handler.
#[derive(Default, Debug, Clone)]
pub struct Event {
    pub transaction_id: TransactionId,
    pub message: Rc<RefCell<Message>>,
    pub error: Option<Error>,
}

// AgentTransaction represents transaction in progress.
// Concurrent access is invalid.
pub(crate) struct AgentTransaction {
    id: TransactionId,
    deadline: Duration,
}

// AGENT_COLLECT_CAP is initial capacity for Agent.Collect slices,
// sufficient to make function zero-alloc in most cases.
const AGENT_COLLECT_CAP: usize = 100;

#[derive(PartialEq, Eq, Hash, Copy, Clone, Default, Debug)]
pub struct TransactionId(pub(crate) [u8; TRANSACTION_ID_SIZE]);

impl TransactionId {
    // NewTransactionID returns new random transaction ID using crypto/rand
    // as source.
    pub fn new() -> Self {
        let mut b = TransactionId([0u8; TRANSACTION_ID_SIZE]);
        rand::thread_rng().fill(&mut b.0);
        b
    }

    pub fn add_to(&self, m: &mut Message) {
        m.transaction_id = *self;
        m.write_transaction_id();
    }
}

// NewAgent initializes and returns new Agent with provided handler.
// If h is nil, the noop_handler will be used.
impl Agent {
    pub fn new(handler: Handler) -> Self {
        Agent {
            transactions: HashMap::new(),
            closed: false,
            handler,
        }
    }

    // stop_with_error removes transaction from list and calls handler with
    // provided error. Can return ErrTransactionNotExists and ErrAgentClosed.
    pub fn stop_with_error(&mut self, id: TransactionId, error: Error) -> Result<(), Error> {
        if self.closed {
            return Err(ERR_AGENT_CLOSED.clone());
        }

        let v = self.transactions.remove(&id);
        if let Some(t) = v {
            (self.handler)(&Event {
                transaction_id: t.id,
                message: Rc::new(RefCell::new(Message::default())),
                error: Some(error),
            });
            Ok(())
        } else {
            Err(ERR_TRANSACTION_NOT_EXISTS.clone())
        }
    }

    // Stop stops transaction by id with ErrTransactionStopped, blocking
    // until handler returns.
    pub fn stop(&mut self, id: TransactionId) -> Result<(), Error> {
        self.stop_with_error(id, ERR_TRANSACTION_STOPPED.clone())
    }

    // Start registers transaction with provided id and deadline.
    // Could return ErrAgentClosed, ErrTransactionExists.
    //
    // Agent handler is guaranteed to be eventually called.
    pub fn start(&mut self, id: TransactionId, deadline: Duration) -> Result<(), Error> {
        if self.closed {
            return Err(ERR_AGENT_CLOSED.clone());
        }
        if self.transactions.contains_key(&id) {
            return Err(ERR_TRANSACTION_EXISTS.clone());
        }

        self.transactions
            .insert(id, AgentTransaction { id, deadline });

        Ok(())
    }

    // Collect terminates all transactions that have deadline before provided
    // time, blocking until all handlers will process ErrTransactionTimeOut.
    // Will return ErrAgentClosed if agent is already closed.
    //
    // It is safe to call Collect concurrently but makes no sense.
    pub fn collect(&mut self, gc_time: Duration) -> Result<(), Error> {
        if self.closed {
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
        for (id, t) in &self.transactions {
            if t.deadline < gc_time {
                to_remove.push(*id);
            }
        }
        // Un-registering timed out transactions.
        for id in &to_remove {
            self.transactions.remove(id);
        }
        // Calling handler does not require locked mutex,
        // reducing lock time.
        //let h = a.handler.clone();
        //a.mux.Unlock()
        // Sending ErrTransactionTimeOut to handler for all transactions,
        // blocking until last one.
        let mut event = Event {
            error: Some(ERR_TRANSACTION_TIME_OUT.clone()),
            ..Default::default()
        };
        for id in to_remove {
            event.transaction_id = id;
            (self.handler)(&event);
        }

        Ok(())
    }

    // process incoming message, synchronously passing it to handler.
    pub fn process(&mut self, m: &Rc<RefCell<Message>>) -> Result<(), Error> {
        if self.closed {
            return Err(ERR_AGENT_CLOSED.clone());
        }

        let e = Event {
            transaction_id: m.borrow().transaction_id,
            message: Rc::clone(&m),
            ..Default::default()
        };

        //h := a.handler
        self.transactions.remove(&e.transaction_id);

        (self.handler)(&e);

        Ok(())
    }

    // set_handler sets agent handler to h.
    pub fn set_handler(&mut self, h: Handler) -> Result<(), Error> {
        if self.closed {
            return Err(ERR_AGENT_CLOSED.clone());
        }
        self.handler = h;

        Ok(())
    }

    // Close terminates all transactions with ErrAgentClosed and renders Agent to
    // closed state.
    pub fn close(&mut self) -> Result<(), Error> {
        if self.closed {
            return Err(ERR_AGENT_CLOSED.clone());
        }

        let mut e = Event {
            error: Some(ERR_AGENT_CLOSED.clone()),
            ..Default::default()
        };

        for id in self.transactions.keys() {
            e.transaction_id = *id;
            (self.handler)(&e);
        }
        self.transactions = HashMap::new();
        self.closed = true;
        self.handler = noop_handler();

        Ok(())
    }
}
