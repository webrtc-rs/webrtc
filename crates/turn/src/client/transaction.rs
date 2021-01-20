use crate::errors::*;

use stun::message::*;

use tokio::net::UdpSocket;
use tokio::sync::{mpsc, Mutex};
use tokio::time::Duration;

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use util::Error;

const MAX_RTX_INTERVAL: Duration = Duration::from_millis(1600);
const MAX_RTX_COUNT: u16 = 7; // total 7 requests (Rc)

async fn on_rtx_timeout(
    conn: &Arc<UdpSocket>,
    tr_map: &Arc<Mutex<TransactionMap>>,
    tr_key: &str,
    n_rtx: u16,
) -> bool {
    let mut tm = tr_map.lock().await;
    let (tr_raw, tr_to) = match tm.find(tr_key) {
        Some(tr) => (tr.raw.clone(), tr.to),
        None => return true, // already gone
    };

    if n_rtx == MAX_RTX_COUNT {
        // all retransmisstions failed
        if let Some(tr) = tm.delete(tr_key) {
            if !tr
                .write_result(TransactionResult {
                    err: Some(Error::new(format!(
                        "{} {}",
                        *ERR_ALL_RETRANSMISSIONS_FAILED, tr_key
                    ))),
                    ..Default::default()
                })
                .await
            {
                log::debug!("no listener for transaction");
            }
        }
        return true;
    }

    log::trace!(
        "retransmitting transaction {} to {} (n_rtx={})",
        tr_key,
        tr_to,
        n_rtx
    );

    if conn.send_to(&tr_raw, tr_to).await.is_err() {
        if let Some(tr) = tm.delete(tr_key) {
            if !tr
                .write_result(TransactionResult {
                    err: Some(Error::new(format!(
                        "{} {}",
                        *ERR_ALL_RETRANSMISSIONS_FAILED, tr_key
                    ))),
                    ..Default::default()
                })
                .await
            {
                log::debug!("no listener for transaction");
            }
        }
        return true;
    }

    false
}

// TransactionResult is a bag of result values of a transaction
#[derive(Debug, Clone)]
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
    pub async fn start_rtx_timer(
        &mut self,
        conn: Arc<UdpSocket>,
        tr_map: Arc<Mutex<TransactionMap>>,
    ) {
        let (timer_ch_tx, mut timer_ch_rx) = mpsc::channel(1);
        self.timer_ch_tx = Some(timer_ch_tx);
        self.n_rtx += 1;
        self.interval *= 2;
        if self.interval > MAX_RTX_INTERVAL {
            self.interval = MAX_RTX_INTERVAL;
        }
        let (n_rtx, interval, key) = (self.n_rtx, self.interval, self.key.clone());

        tokio::spawn(async move {
            let mut done = false;
            while !done {
                let timer = tokio::time::sleep(interval);
                tokio::pin!(timer);

                tokio::select! {
                    _ = timer.as_mut() => done = on_rtx_timeout(&conn, &tr_map, &key, n_rtx).await,
                    _ = timer_ch_rx.recv() => done = true,
                }
            }
        });
    }

    // stop_rtx_timer stop the transaction timer
    pub fn stop_rtx_timer(&mut self) {
        if self.timer_ch_tx.is_some() {
            self.timer_ch_tx.take();
        }
    }

    // write_result writes the result to the result channel
    pub async fn write_result(&self, res: TransactionResult) -> bool {
        if let Some(result_ch) = &self.result_ch_tx {
            result_ch.send(res).await.is_ok()
        } else {
            false
        }
    }

    pub fn get_result_channel(&mut self) -> Option<mpsc::Receiver<TransactionResult>> {
        self.result_ch_rx.take()
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
    pub fn delete(&mut self, key: &str) -> Option<Transaction> {
        self.tr_map.remove(key)
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
