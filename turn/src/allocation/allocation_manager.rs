#[cfg(test)]
mod allocation_manager_test;

use std::collections::HashMap;

use futures::future;
use stun::textattrs::Username;
use tokio::sync::mpsc;
use util::Conn;

use super::*;
use crate::error::*;
use crate::relay::*;

/// `ManagerConfig` a bag of config params for `Manager`.
pub struct ManagerConfig {
    pub relay_addr_generator: Box<dyn RelayAddressGenerator + Send + Sync>,
    pub alloc_close_notify: Option<mpsc::Sender<AllocationInfo>>,
}

/// `Manager` is used to hold active allocations.
pub struct Manager {
    allocations: AllocationMap,
    reservations: Arc<Mutex<HashMap<String, u16>>>,
    relay_addr_generator: Box<dyn RelayAddressGenerator + Send + Sync>,
    alloc_close_notify: Option<mpsc::Sender<AllocationInfo>>,
}

impl Manager {
    /// Creates a new [`Manager`].
    pub fn new(config: ManagerConfig) -> Self {
        Manager {
            allocations: Arc::new(Mutex::new(HashMap::new())),
            reservations: Arc::new(Mutex::new(HashMap::new())),
            relay_addr_generator: config.relay_addr_generator,
            alloc_close_notify: config.alloc_close_notify,
        }
    }

    /// Closes this [`manager`] and closes all [`Allocation`]s it manages.
    pub async fn close(&self) -> Result<()> {
        let allocations = self.allocations.lock().await;
        for a in allocations.values() {
            a.close().await?;
        }
        Ok(())
    }

    /// Returns the information about the all [`Allocation`]s associated with
    /// the specified [`FiveTuple`]s.
    pub async fn get_allocations_info(
        &self,
        five_tuples: Option<Vec<FiveTuple>>,
    ) -> HashMap<FiveTuple, AllocationInfo> {
        let mut infos = HashMap::new();

        let guarded = self.allocations.lock().await;

        guarded.iter().for_each(|(five_tuple, alloc)| {
            if five_tuples.is_none() || five_tuples.as_ref().unwrap().contains(five_tuple) {
                infos.insert(
                    *five_tuple,
                    AllocationInfo::new(
                        *five_tuple,
                        alloc.username.text.clone(),
                        #[cfg(feature = "metrics")]
                        alloc.relayed_bytes.load(Ordering::Acquire),
                    ),
                );
            }
        });

        infos
    }

    /// Fetches the [`Allocation`] matching the passed [`FiveTuple`].
    pub async fn get_allocation(&self, five_tuple: &FiveTuple) -> Option<Arc<Allocation>> {
        let allocations = self.allocations.lock().await;
        allocations.get(five_tuple).cloned()
    }

    /// Creates a new [`Allocation`] and starts relaying.
    pub async fn create_allocation(
        &self,
        five_tuple: FiveTuple,
        turn_socket: Arc<dyn Conn + Send + Sync>,
        requested_port: u16,
        lifetime: Duration,
        username: Username,
        use_ipv4: bool,
    ) -> Result<Arc<Allocation>> {
        if lifetime == Duration::from_secs(0) {
            return Err(Error::ErrLifetimeZero);
        }

        if self.get_allocation(&five_tuple).await.is_some() {
            return Err(Error::ErrDupeFiveTuple);
        }

        let (relay_socket, relay_addr) = self
            .relay_addr_generator
            .allocate_conn(use_ipv4, requested_port)
            .await?;
        let mut a = Allocation::new(
            turn_socket,
            relay_socket,
            relay_addr,
            five_tuple,
            username,
            self.alloc_close_notify.clone(),
        );
        a.allocations = Some(Arc::clone(&self.allocations));

        log::debug!("listening on relay addr: {:?}", a.relay_addr);
        a.start(lifetime).await;
        a.packet_handler().await;

        let a = Arc::new(a);
        {
            let mut allocations = self.allocations.lock().await;
            allocations.insert(five_tuple, Arc::clone(&a));
        }

        Ok(a)
    }

    /// Removes an [`Allocation`].
    pub async fn delete_allocation(&self, five_tuple: &FiveTuple) {
        let allocation = self.allocations.lock().await.remove(five_tuple);

        if let Some(a) = allocation {
            if let Err(err) = a.close().await {
                log::error!("Failed to close allocation: {}", err);
            }
        }
    }

    /// Deletes the [`Allocation`]s according to the specified username `name`.
    pub async fn delete_allocations_by_username(&self, name: &str) {
        let to_delete = {
            let mut allocations = self.allocations.lock().await;

            let mut to_delete = Vec::new();

            // TODO(logist322): Use `.drain_filter()` once stabilized.
            allocations.retain(|_, allocation| {
                let match_name = allocation.username.text == name;

                if match_name {
                    to_delete.push(Arc::clone(allocation));
                }

                !match_name
            });

            to_delete
        };

        future::join_all(to_delete.iter().map(|a| async move {
            if let Err(err) = a.close().await {
                log::error!("Failed to close allocation: {}", err);
            }
        }))
        .await;
    }

    /// Stores the reservation for the token+port.
    pub async fn create_reservation(&self, reservation_token: String, port: u16) {
        let reservations = Arc::clone(&self.reservations);
        let reservation_token2 = reservation_token.clone();

        tokio::spawn(async move {
            let sleep = tokio::time::sleep(Duration::from_secs(30));
            tokio::pin!(sleep);
            tokio::select! {
                _ = &mut sleep => {
                    let mut reservations = reservations.lock().await;
                    reservations.remove(&reservation_token2);
                },
            }
        });

        let mut reservations = self.reservations.lock().await;
        reservations.insert(reservation_token, port);
    }

    /// Returns the port for a given reservation if it exists.
    pub async fn get_reservation(&self, reservation_token: &str) -> Option<u16> {
        let reservations = self.reservations.lock().await;
        reservations.get(reservation_token).copied()
    }

    /// Returns a random un-allocated udp4 port.
    pub async fn get_random_even_port(&self) -> Result<u16> {
        let (_, addr) = self.relay_addr_generator.allocate_conn(true, 0).await?;
        Ok(addr.port())
    }
}
