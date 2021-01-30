#[cfg(test)]
mod server_test;

pub mod config;
pub mod request;

use crate::allocation::allocation_manager::*;
use crate::auth::AuthHandler;
use crate::proto::lifetime::DEFAULT_LIFETIME;
use config::*;
use request::*;

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{Duration, Instant};

use util::{Conn, Error};

const INBOUND_MTU: usize = 1500;

// Server is an instance of the Pion TURN Server
pub struct Server {
    auth_handler: Arc<Box<dyn AuthHandler + Send + Sync>>,
    realm: String,
    channel_bind_timeout: Duration,
    nonces: Arc<Mutex<HashMap<String, Instant>>>,
}

impl Server {
    // creates the TURN server
    pub async fn new(config: ServerConfig) -> Result<Self, Error> {
        config.validate()?;

        let mut s = Server {
            auth_handler: config.auth_handler,
            realm: config.realm,
            channel_bind_timeout: config.channel_bind_timeout,
            nonces: Arc::new(Mutex::new(HashMap::new())),
        };

        if s.channel_bind_timeout == Duration::from_secs(0) {
            s.channel_bind_timeout = DEFAULT_LIFETIME;
        }

        for p in config.conn_configs.into_iter() {
            let nonces = Arc::clone(&s.nonces);
            let auth_handler = Arc::clone(&s.auth_handler);
            let realm = s.realm.clone();
            let channel_bind_timeout = s.channel_bind_timeout;

            tokio::spawn(async move {
                let allocation_manager = Arc::new(Manager::new(ManagerConfig {
                    relay_addr_generator: p.relay_addr_generator,
                }));

                let _ = Server::read_loop(
                    p.conn,
                    allocation_manager,
                    nonces,
                    auth_handler,
                    realm,
                    channel_bind_timeout,
                )
                .await;
            });
        }

        Ok(s)
    }

    async fn read_loop(
        conn: Arc<dyn Conn + Send + Sync>,
        allocation_manager: Arc<Manager>,
        nonces: Arc<Mutex<HashMap<String, Instant>>>,
        auth_handler: Arc<Box<dyn AuthHandler + Send + Sync>>,
        realm: String,
        channel_bind_timeout: Duration,
    ) {
        let mut buf = vec![0u8; INBOUND_MTU];

        loop {
            //TODO: gracefully exit loop
            let (n, addr) = match conn.recv_from(&mut buf).await {
                Ok((n, addr)) => (n, addr),
                Err(err) => {
                    log::debug!("exit read loop on error: {}", err);
                    break;
                }
            };

            let mut r = Request {
                conn: Arc::clone(&conn),
                src_addr: addr,
                buff: buf[..n].to_vec(),
                allocation_manager: Arc::clone(&allocation_manager),
                nonces: Arc::clone(&nonces),
                auth_handler: Arc::clone(&auth_handler),
                realm: realm.clone(),
                channel_bind_timeout,
            };

            if let Err(err) = r.handle_request().await {
                log::error!("error when handling datagram: {}", err);
            }
        }

        let _ = allocation_manager.close().await;
    }

    // Close stops the TURN Server. It cleans up any associated state and closes all connections it is managing
    pub fn close(&self) -> Result<(), Error> {
        Ok(())
    }
}
