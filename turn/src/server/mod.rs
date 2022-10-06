#[cfg(test)]
mod server_test;

pub mod config;
pub mod request;

use crate::{
    allocation::allocation_manager::*, auth::AuthHandler, error::*,
    proto::lifetime::DEFAULT_LIFETIME,
};
use config::*;
use request::*;

use std::{collections::HashMap, sync::Arc};

use tokio::{
    sync::{
        broadcast::{self, error::RecvError},
        mpsc, oneshot, Mutex,
    },
    time::{Duration, Instant},
};
use util::Conn;

const INBOUND_MTU: usize = 1500;

/// Server is an instance of the TURN Server
pub struct Server {
    auth_handler: Arc<dyn AuthHandler + Send + Sync>,
    realm: String,
    channel_bind_timeout: Duration,
    pub(crate) nonces: Arc<Mutex<HashMap<String, Instant>>>,
    command_tx: Mutex<Option<broadcast::Sender<Command>>>,
}

impl Server {
    /// creates the TURN server
    pub async fn new(config: ServerConfig) -> Result<Self> {
        config.validate()?;

        let (command_tx, _) = broadcast::channel(16);
        let mut s = Server {
            auth_handler: config.auth_handler,
            realm: config.realm,
            channel_bind_timeout: config.channel_bind_timeout,
            nonces: Arc::new(Mutex::new(HashMap::new())),
            command_tx: Mutex::new(Some(command_tx.clone())),
        };

        if s.channel_bind_timeout == Duration::from_secs(0) {
            s.channel_bind_timeout = DEFAULT_LIFETIME;
        }

        for p in config.conn_configs.into_iter() {
            let nonces = Arc::clone(&s.nonces);
            let auth_handler = Arc::clone(&s.auth_handler);
            let realm = s.realm.clone();
            let channel_bind_timeout = s.channel_bind_timeout;
            let handle_rx = command_tx.subscribe();
            let conn = p.conn;
            let allocation_manager = Arc::new(Manager::new(ManagerConfig {
                relay_addr_generator: p.relay_addr_generator,
            }));

            tokio::spawn(Server::read_loop(
                conn,
                allocation_manager,
                nonces,
                auth_handler,
                realm,
                channel_bind_timeout,
                handle_rx,
            ));
        }

        Ok(s)
    }

    /// Deletes all existing [`crate::allocation::Allocation`]s by the provided `username`.
    pub async fn delete_allocations_by_username(&self, username: String) -> Result<()> {
        let tx = self.command_tx.lock().await.clone();
        if let Some(tx) = tx {
            let (closed_tx, closed_rx) = mpsc::channel(1);
            tx.send(Command::DeleteAllocations(username, Arc::new(closed_rx)))
                .map_err(|_| Error::ErrClosed)?;

            closed_tx.closed().await;

            Ok(())
        } else {
            Err(Error::ErrClosed)
        }
    }

    async fn read_loop(
        conn: Arc<dyn Conn + Send + Sync>,
        allocation_manager: Arc<Manager>,
        nonces: Arc<Mutex<HashMap<String, Instant>>>,
        auth_handler: Arc<dyn AuthHandler + Send + Sync>,
        realm: String,
        channel_bind_timeout: Duration,
        mut handle_rx: broadcast::Receiver<Command>,
    ) {
        let mut buf = vec![0u8; INBOUND_MTU];

        let (mut close_tx, mut close_rx) = oneshot::channel::<()>();

        tokio::spawn({
            let allocation_manager = Arc::clone(&allocation_manager);

            async move {
                loop {
                    match handle_rx.recv().await {
                        Ok(Command::DeleteAllocations(name, _)) => {
                            allocation_manager
                                .delete_allocations_by_username(name.as_str())
                                .await;
                            continue;
                        }
                        Err(RecvError::Closed) | Ok(Command::Close(_)) => {
                            close_rx.close();
                            break;
                        }
                        Err(RecvError::Lagged(n)) => {
                            log::warn!("Turn server has lagged by {} messages", n);
                            continue;
                        }
                    }
                }
            }
        });

        loop {
            let (n, addr) = tokio::select! {
                v = conn.recv_from(&mut buf) => {
                    match v {
                        Ok(v) => v,
                        Err(err) => {
                            log::debug!("exit read loop on error: {}", err);
                            break;
                        }
                    }
                },
                _ = close_tx.closed() => break
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
        let _ = conn.close().await;
    }

    /// Close stops the TURN Server. It cleans up any associated state and closes all connections it is managing
    pub async fn close(&self) -> Result<()> {
        let tx = self.command_tx.lock().await.take();
        if let Some(tx) = tx {
            if tx.receiver_count() == 0 {
                return Ok(());
            }

            let (closed_tx, closed_rx) = mpsc::channel(1);
            let _ = tx.send(Command::Close(Arc::new(closed_rx)));
            closed_tx.closed().await
        }

        Ok(())
    }
}

/// The protocol to communicate between the [`Server`]'s public methods
/// and the tasks spawned in the [`read_loop`] method.
#[derive(Clone)]
enum Command {
    /// Command to delete [`crate::allocation::Allocation`] by provided
    /// `username`.
    DeleteAllocations(String, Arc<mpsc::Receiver<()>>),

    /// Command to close the [`Server`].
    Close(Arc<mpsc::Receiver<()>>),
}
