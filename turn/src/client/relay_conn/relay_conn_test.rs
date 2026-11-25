use std::net::{IpAddr, Ipv4Addr};
use std::str::FromStr;

use tokio::net::UdpSocket;
use util::vnet::net::*;

use super::*;
use crate::auth::*;
use crate::client::*;
use crate::error::Result;
use crate::relay::relay_static::*;
use crate::server::config::*;
use crate::server::*;

struct DummyRelayConnObserver {
    turn_server_addr: String,
    username: Username,
    realm: Realm,
}

#[async_trait]
impl RelayConnObserver for DummyRelayConnObserver {
    fn turn_server_addr(&self) -> String {
        self.turn_server_addr.clone()
    }

    fn username(&self) -> Username {
        self.username.clone()
    }

    fn realm(&self) -> Realm {
        self.realm.clone()
    }

    async fn write_to(&self, _data: &[u8], _to: &str) -> std::result::Result<usize, util::Error> {
        Ok(0)
    }

    async fn perform_transaction(
        &mut self,
        _msg: &Message,
        _to: &str,
        _dont_wait: bool,
    ) -> Result<TransactionResult> {
        Err(Error::ErrFakeErr)
    }
}

#[tokio::test]
async fn test_relay_conn() -> Result<()> {
    let obs = DummyRelayConnObserver {
        turn_server_addr: String::new(),
        username: Username::new(ATTR_USERNAME, "username".to_owned()),
        realm: Realm::new(ATTR_REALM, "realm".to_owned()),
    };

    let (_read_ch_tx, read_ch_rx) = mpsc::channel(100);

    let config = RelayConnConfig {
        relayed_addr: SocketAddr::new(Ipv4Addr::new(0, 0, 0, 0).into(), 0),
        integrity: MessageIntegrity::default(),
        nonce: Nonce::new(ATTR_NONCE, "nonce".to_owned()),
        lifetime: Duration::from_secs(0),
        binding_mgr: Arc::new(Mutex::new(BindingManager::new())),
        read_ch_rx: Arc::new(Mutex::new(read_ch_rx)),
    };

    let rc = RelayConn::new(Arc::new(Mutex::new(obs)), config).await;

    let rci = rc.relay_conn.lock().await;
    let (bind_addr, bind_number) = {
        let mut bm = rci.binding_mgr.lock().await;
        let b = bm
            .create(SocketAddr::new(Ipv4Addr::new(127, 0, 0, 1).into(), 1234))
            .unwrap();
        (b.addr, b.number)
    };

    //let binding_mgr = Arc::clone(&rci.binding_mgr);
    let rc_obs = Arc::clone(&rci.obs);
    let nonce = rci.nonce.clone();
    let integrity = rci.integrity.clone();

    if let Err(err) =
        RelayConnInternal::bind(rc_obs, bind_addr, bind_number, nonce, integrity).await
    {
        assert!(Error::ErrUnexpectedResponse != err);
    } else {
        panic!("should fail");
    }

    Ok(())
}

struct TestAuthHandler;
impl AuthHandler for TestAuthHandler {
    fn auth_handle(&self, username: &str, realm: &str, _src_addr: SocketAddr) -> Result<Vec<u8>> {
        Ok(generate_auth_key(username, realm, "pass"))
    }
}

// Create an allocation, and then delete it on the server
// The next allocation refresh gets 437 (Allocation Mismatch)
// and the relay stops refreshing
#[tokio::test]
async fn test_relay_conn_allocation_lost() -> Result<()> {
    let conn = Arc::new(UdpSocket::bind("0.0.0.0:0").await?);
    let server_port = conn.local_addr()?.port();

    let server = Server::new(ServerConfig {
        conn_configs: vec![ConnConfig {
            conn,
            relay_addr_generator: Box::new(RelayAddressGeneratorStatic {
                relay_address: IpAddr::from_str("127.0.0.1")?,
                address: "0.0.0.0".to_owned(),
                net: Arc::new(Net::new(None)),
            }),
        }],
        realm: "webrtc.rs".to_owned(),
        auth_handler: Arc::new(TestAuthHandler {}),
        channel_bind_timeout: Duration::from_secs(0),
        alloc_close_notify: None,
    })
    .await?;

    let conn = Arc::new(UdpSocket::bind("0.0.0.0:0").await?);

    let client = Client::new(ClientConfig {
        stun_serv_addr: format!("127.0.0.1:{server_port}"),
        turn_serv_addr: format!("127.0.0.1:{server_port}"),
        username: "foo".to_owned(),
        password: "pass".to_owned(),
        realm: String::new(),
        software: String::new(),
        rto_in_ms: 0,
        conn,
        vnet: None,
    })
    .await?;

    client.listen().await?;

    let allocation = client.allocate().await?;
    let relay_conn = &allocation
        .as_any()
        .downcast_ref::<RelayConn<ClientInternal>>()
        .unwrap()
        .relay_conn;

    server
        .delete_allocations_by_username("foo".to_owned())
        .await?;

    relay_conn
        .lock()
        .await
        .on_timeout(TimerIdRefresh::Alloc)
        .await;
    assert!(relay_conn.lock().await.allocation_lost);

    // Shutdown
    client.close().await?;
    server.close().await?;

    Ok(())
}
