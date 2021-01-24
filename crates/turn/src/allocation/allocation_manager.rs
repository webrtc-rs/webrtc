use super::*;
use crate::errors::*;

use std::collections::HashMap;
use std::net::SocketAddr;

use util::{Conn, Error};

type AllocateConnFn =
    fn(network: String, requestedPort: u16) -> Result<(Box<dyn Conn>, SocketAddr), Error>;

// ManagerConfig a bag of config params for Manager.
pub struct ManagerConfig {
    allocate_conn: AllocateConnFn,
}

// Manager is used to hold active allocations
pub struct Manager {
    allocations: HashMap<String, Allocation>,
    reservations: Arc<Mutex<HashMap<String, u16>>>,
    allocate_conn: AllocateConnFn,
}

impl Manager {
    // creates a new instance of Manager.
    pub fn new(config: ManagerConfig) -> Self {
        Manager {
            allocations: HashMap::new(),
            reservations: Arc::new(Mutex::new(HashMap::new())),
            allocate_conn: config.allocate_conn,
        }
    }

    // Close closes the manager and closes all allocations it manages
    pub async fn close(&mut self) -> Result<(), Error> {
        for a in self.allocations.values_mut() {
            a.close().await?;
        }
        Ok(())
    }

    // get_allocation fetches the allocation matching the passed FiveTuple
    pub fn get_allocation(&self, five_tuple: &FiveTuple) -> Option<&Allocation> {
        self.allocations.get(&five_tuple.fingerprint())
    }

    // create_allocation creates a new allocation and starts relaying
    pub fn create_allocation(
        &self,
        five_tuple: FiveTuple,
        turn_socket: impl Conn,
        _requested_port: u16,
        lifetime: Duration,
    ) -> Result<Allocation, Error> {
        if lifetime == Duration::from_secs(0) {
            return Err(ERR_LIFETIME_ZERO.to_owned());
        }

        if self.get_allocation(&five_tuple).is_some() {
            return Err(ERR_DUPE_FIVE_TUPLE.to_owned());
        }

        let a = Allocation::new(turn_socket, five_tuple);

        /*TODO:
        conn, relayAddr, err := m.allocatePacketConn("udp4", requested_port)
        if err != nil {
            return nil, err
        }

        a.RelaySocket = conn
        a.RelayAddr = relayAddr

        m.log.Debugf("listening on relay addr: %s", a.RelayAddr.String())

        a.lifetimeTimer = time.AfterFunc(lifetime, func() {
            m.delete_allocation(a.fiveTuple)
        })

        m.lock.Lock()
        m.allocations[fiveTuple.Fingerprint()] = a
        m.lock.Unlock()

        go a.packetHandler(m)
        return a, nil
        */

        Ok(a)
    }

    // delete_allocation removes an allocation
    pub async fn delete_allocation(&mut self, five_tuple: &FiveTuple) {
        let fingerprint = five_tuple.fingerprint();

        let allocation = self.allocations.remove(&fingerprint);
        if let Some(mut a) = allocation {
            if let Err(err) = a.close().await {
                log::error!("Failed to close allocation: {}", err);
            }
        }
    }

    // create_reservation stores the reservation for the token+port
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

    // get_reservation returns the port for a given reservation if it exists
    pub async fn get_reservation(&self, reservation_token: &str) -> Option<u16> {
        let reservations = self.reservations.lock().await;
        if let Some(port) = reservations.get(reservation_token) {
            Some(*port)
        } else {
            None
        }
    }

    // get_random_even_port returns a random un-allocated udp4 port
    pub fn get_random_even_port(&self) -> Result<u16, Error> {
        let (_, addr) = (self.allocate_conn)("udp4".to_owned(), 0)?;
        if addr.port() % 2 == 1 {
            self.get_random_even_port()
        } else {
            Ok(addr.port())
        }
    }
}
