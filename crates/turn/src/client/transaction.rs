use crate::errors::*;

use stun::message::*;

use tokio::sync::mpsc;
use tokio::time::Duration;

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use util::Error;

const MAX_RTX_INTERVAL: Duration = Duration::from_millis(1600);

pub trait RtxTimer {
    fn on_timeout(&mut self, tr_key: String, n_rtx: u16);
}

// TransactionResult is a bag of result values of a transaction
#[derive(Debug, Clone)]
pub struct TransactionResult {
    pub msg: Message,
    pub from: SocketAddr,
    pub retries: u16,
}

impl Default for TransactionResult {
    fn default() -> Self {
        TransactionResult {
            msg: Message::default(),
            from: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 0),
            retries: 0,
        }
    }
}

// TransactionConfig is a set of config params used by NewTransaction
#[derive(Debug)]
pub struct TransactionConfig {
    pub key: String,
    pub raw: Vec<u8>,
    pub to: SocketAddr,
    pub interval: Duration,
    pub ignore_result: bool, // true to throw away the result of this transaction (it will not be readable using wait_for_result)
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
    pub key: String,
    pub raw: Vec<u8>,
    pub to: SocketAddr,
    pub n_rtx: u16,
    pub interval: Duration,
    //pub timer: Option<Sleep>,
    timer_ch_tx: Option<mpsc::Sender<()>>,
    result_ch_tx: Option<mpsc::Sender<TransactionResult>>,
    result_ch_rx: Option<mpsc::Receiver<TransactionResult>>,
}

impl Default for Transaction {
    fn default() -> Self {
        Transaction {
            key: String::new(),
            raw: vec![],
            to: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 0),
            n_rtx: 0,
            interval: Duration::from_secs(0),
            //timer: None,
            timer_ch_tx: None,
            result_ch_tx: None,
            result_ch_rx: None,
        }
    }
}

impl Transaction {
    // NewTransaction creates a new instance of Transaction
    pub fn new(config: TransactionConfig) -> Self {
        let (result_ch_tx, result_ch_rx) = if !config.ignore_result {
            let (tx, rx) = mpsc::channel(1);
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };

        Transaction {
            key: config.key,
            raw: config.raw,
            to: config.to,
            interval: config.interval,
            result_ch_tx,
            result_ch_rx,
            ..Default::default()
        }
    }

    // start_rtx_timer starts the transaction timer
    pub async fn start_rtx_timer(&mut self, t: &mut Box<dyn RtxTimer>) {
        let (timer_ch_tx, mut timer_ch_rx) = mpsc::channel(1);
        self.timer_ch_tx = Some(timer_ch_tx);

        let timer = tokio::time::sleep(self.interval);
        tokio::pin!(timer);

        tokio::select! {
            _ = timer.as_mut() => {
                self.n_rtx+=1;
                let n_rtx = self.n_rtx;
                self.interval *= 2;
                if self.interval > MAX_RTX_INTERVAL {
                    self.interval = MAX_RTX_INTERVAL;
                }
                t.on_timeout(self.key.clone(), n_rtx);
            }
            _ = timer_ch_rx.recv() => {
                self.timer_ch_tx.take();
            }
        }
    }

    // stop_rtx_timer stop the transaction timer
    pub fn stop_rtx_timer(&mut self) {
        if self.timer_ch_tx.is_some() {
            self.timer_ch_tx.take();
        }
    }

    // write_result writes the result to the result channel
    pub async fn write_result(&mut self, res: TransactionResult) -> bool {
        if let Some(result_ch) = &self.result_ch_tx {
            let _ = result_ch.send(res).await;
            true
        } else {
            false
        }
    }

    // wait_for_result waits for the transaction result
    pub async fn wait_for_result(&mut self) -> Result<TransactionResult, Error> {
        if let Some(result_ch_rx) = &mut self.result_ch_rx {
            match result_ch_rx.recv().await {
                Some(tr) => Ok(tr),
                None => Err(ERR_TRANSACTION_CLOSED.to_owned()),
            }
        } else {
            Err(ERR_WAIT_FOR_RESULT_ON_NON_RESULT_TRANSACTION.to_owned())
        }
    }

    // Close closes the transaction
    pub fn close(&mut self) {
        if self.result_ch_tx.is_some() {
            self.result_ch_tx.take();
        }
    }

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

    pub fn get(&mut self, key: &str) -> Option<&mut Transaction> {
        self.tr_map.get_mut(key)
    }

    // Delete deletes a transaction by its key
    pub fn delete(&mut self, key: &str) {
        self.tr_map.remove(key);
    }

    // close_and_delete_all closes and deletes all transactions
    pub fn close_and_delete_all(&mut self) {
        for tr in self.tr_map.values_mut() {
            tr.close();
        }
        self.tr_map.clear();
    }

    // Size returns the length of the transaction map
    pub fn size(&self) -> usize {
        self.tr_map.len()
    }
}
