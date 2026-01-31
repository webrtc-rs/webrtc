#![warn(rust_2018_idioms)]
#![allow(dead_code)]

pub mod utilities;

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::sync::Arc;

use dtls::Error;
use tokio::sync::Mutex;
use util::Conn;

const BUF_SIZE: usize = 8192;

/// Hub is a helper to handle one to many chat
#[derive(Default)]
pub struct Hub {
    conns: Arc<Mutex<HashMap<String, Arc<dyn Conn + Send + Sync>>>>,
}

impl Hub {
    /// new builds a new hub
    pub fn new() -> Self {
        Hub {
            conns: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// register adds a new conn to the Hub
    pub async fn register(&self, conn: Arc<dyn Conn + Send + Sync>) {
        println!("Connected to {}", conn.remote_addr().unwrap());

        if let Some(remote_addr) = conn.remote_addr() {
            let mut conns = self.conns.lock().await;
            conns.insert(remote_addr.to_string(), Arc::clone(&conn));
        }

        let conns = Arc::clone(&self.conns);
        tokio::spawn(async move {
            let _ = Hub::read_loop(conns, conn).await;
        });
    }

    async fn read_loop(
        conns: Arc<Mutex<HashMap<String, Arc<dyn Conn + Send + Sync>>>>,
        conn: Arc<dyn Conn + Send + Sync>,
    ) -> Result<(), Error> {
        let mut b = vec![0u8; BUF_SIZE];

        while let Ok(n) = conn.recv(&mut b).await {
            let msg = String::from_utf8(b[..n].to_vec())?;
            print!("Got message: {msg}");
        }

        Hub::unregister(conns, conn).await
    }

    async fn unregister(
        conns: Arc<Mutex<HashMap<String, Arc<dyn Conn + Send + Sync>>>>,
        conn: Arc<dyn Conn + Send + Sync>,
    ) -> Result<(), Error> {
        if let Some(remote_addr) = conn.remote_addr() {
            {
                let mut cs = conns.lock().await;
                cs.remove(&remote_addr.to_string());
            }

            if let Err(err) = conn.close().await {
                println!("Failed to disconnect: {remote_addr} with err {err}");
            } else {
                println!("Disconnected: {remote_addr} ");
            }
        }

        Ok(())
    }

    async fn broadcast(&self, msg: &[u8]) {
        let conns = self.conns.lock().await;
        for conn in conns.values() {
            if let Err(err) = conn.send(msg).await {
                println!(
                    "Failed to write message to {:?}: {}",
                    conn.remote_addr(),
                    err
                );
            }
        }
    }

    /// Chat starts the stdin readloop to dispatch messages to the hub
    pub async fn chat(&self) {
        let input = std::io::stdin();
        let mut reader = BufReader::new(input.lock());
        loop {
            let mut msg = String::new();
            match reader.read_line(&mut msg) {
                Ok(0) => return,
                Err(err) => {
                    println!("stdin read err: {err}");
                    return;
                }
                _ => {}
            };
            if msg.trim() == "exit" {
                return;
            }
            self.broadcast(msg.as_bytes()).await;
        }
    }
}
