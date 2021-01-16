use stun::message::*;

use tokio::sync::mpsc;
use tokio::time::{Duration, Sleep};

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use std::collections::HashMap;
use util::Error;

const MAX_RTX_INTERVAL: Duration = Duration::from_millis(1600);

// TransactionResult is a bag of result values of a transaction
#[derive(Debug)]
pub struct TransactionResult {
    pub msg: Message,
    pub from: SocketAddr,
    pub retries: u16,
    pub err: Option<Error>,
}

impl Default for TransactionResult {
    fn default() -> Self {
        TransactionResult {
            msg: Message::default(),
            from: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 0),
            retries: 0,
            err: None,
        }
    }
}

// TransactionConfig is a set of config params used by NewTransaction
#[derive(Debug)]
pub struct TransactionConfig {
    key: String,
    raw: Vec<u8>,
    to: SocketAddr,
    interval: Duration,
    ignore_result: bool, // true to throw away the result of this transaction (it will not be readable using WaitForResult)
}

impl Default for TransactionConfig {
    fn default() -> Self {
        TransactionConfig {
            key: String::new(),
            raw: vec![],
            to: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 0),
            interval: Duration::from_secs(0),
            ignore_result: false,
        }
    }
}

// Transaction represents a transaction
#[derive(Debug)]
pub struct Transaction {
    key: String,          // read-only
    raw: Vec<u8>,         // read-only
    to: SocketAddr,       // read-only
    n_rtx: u16,           // modified only by the timer thread
    interval: Duration,   // modified only by the timer thread
    timer: Option<Sleep>, // thread-safe, set only by the creator, and stopper
    result_ch: Option<mpsc::Sender<TransactionResult>>, // thread-safe
                          //mutex    :sync.RWMutex
}

impl Default for Transaction {
    fn default() -> Self {
        Transaction {
            key: String::new(), // read-only
            raw: vec![],        // read-only
            to: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 0),
            n_rtx: 0,                         // modified only by the timer thread
            interval: Duration::from_secs(0), // modified only by the timer thread
            timer: None,                      // thread-safe, set only by the creator, and stopper
            result_ch: None,
        }
    }
}

impl Transaction {
    // NewTransaction creates a new instance of Transaction
    pub fn new(config: TransactionConfig) -> Self {
        //TODO: var result_ch chan TransactionResult
        //if !config.IgnoreResult {
        //    result_ch = make(chan TransactionResult)
        //}

        Transaction {
            key: config.key,           // read-only
            raw: config.raw,           // read-only
            to: config.to,             // read-only
            interval: config.interval, // modified only by the timer thread
            //TODO: result_ch: result_ch,        // thread-safe
            ..Default::default()
        }
    }

    // StartRtxTimer starts the transaction timer
    /*TODO: func (t *Transaction) StartRtxTimer(onTimeout func(trKey string, n_rtx int)) {
        t.mutex.Lock()
        defer t.mutex.Unlock()

        t.timer = time.AfterFunc(t.interval, func() {
            t.mutex.Lock()
            t.n_rtx++
            n_rtx := t.n_rtx
            t.interval *= 2
            if t.interval > MAX_RTX_INTERVAL {
                t.interval = MAX_RTX_INTERVAL
            }
            t.mutex.Unlock()
            onTimeout(t.key, n_rtx)
        })
    }

    // StopRtxTimer stop the transaction timer
    func (t *Transaction) StopRtxTimer() {
        t.mutex.Lock()
        defer t.mutex.Unlock()

        if t.timer != nil {
            t.timer.Stop()
        }
    }
    */

    /*
    // write_result writes the result to the result channel
    pub async fn write_result(&mut self, res: TransactionResult) -> bool {
        if let Some(result_ch) = &self.result_ch {
            let _ = result_ch.send(res).await;
            true
        } else {
            false
        }
    }

    // WaitForResult waits for the transaction result
    pub async fn WaitForResult(&self) -> TransactionResult {
        if let Some(result_ch) = &self.result_ch {
            let result = result_ch.recv().await;
            if !ok {
                result.Err = errTransactionClosed
            }
            return result;
        } else {
            return TransactionResult {
                Err: errWaitForResultOnNonResultTransaction,
            };
        }
    }

    // Close closes the transaction
    func (t *Transaction) Close() {
        if t.result_ch != nil {
            close(t.result_ch)
        }
    }

    */

    // retries returns the number of retransmission it has made
    pub fn retries(&self) -> u16 {
        self.n_rtx
    }
}

// TransactionMap is a thread-safe transaction map
#[derive(Default, Debug)]
pub struct TransactionMap {
    tr_map: HashMap<String, Transaction>,
}

impl TransactionMap {
    // NewTransactionMap create a new instance of the transaction map
    pub fn new() -> TransactionMap {
        TransactionMap {
            tr_map: HashMap::new(),
        }
    }

    // Insert inserts a trasaction to the map
    pub fn insert(&mut self, key: String, tr: Transaction) -> bool {
        self.tr_map.insert(key, tr);
        true
    }

    // Find looks up a transaction by its key
    pub fn find(&self, key: &str) -> Option<&Transaction> {
        self.tr_map.get(key)
    }

    // Delete deletes a transaction by its key
    pub fn delete(&mut self, key: &str) {
        self.tr_map.remove(key);
    }

    // CloseAndDeleteAll closes and deletes all transactions
    /*TODO: pub fn CloseAndDeleteAll(&mut self) {
        for trKey, tr := range m.tr_map {
            tr.Close()
            delete(m.tr_map, trKey)
        }
    }*/

    // Size returns the length of the transaction map
    pub fn size(&self) -> usize {
        self.tr_map.len()
    }
}
