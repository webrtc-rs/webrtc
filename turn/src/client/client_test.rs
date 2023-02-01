use super::*;
use crate::auth::*;
use crate::relay::relay_static::*;
use crate::server::{config::*, *};

use std::net::IpAddr;
use tokio::net::UdpSocket;
use tokio::time::Duration;

use util::vnet::net::*;

async fn create_listening_test_client(rto_in_ms: u16) -> Result<Client> {
    let conn = UdpSocket::bind("0.0.0.0:0").await?;

    let c = Client::new(ClientConfig {
        stun_serv_addr: String::new(),
        turn_serv_addr: String::new(),
        username: String::new(),
        password: String::new(),
        realm: String::new(),
        software: "TEST SOFTWARE".to_owned(),
        rto_in_ms,
        conn: Arc::new(conn),
        vnet: None,
    })
    .await?;

    c.listen().await?;

    Ok(c)
}

async fn create_listening_test_client_with_stun_serv() -> Result<Client> {
    let conn = UdpSocket::bind("0.0.0.0:0").await?;

    let c = Client::new(ClientConfig {
        stun_serv_addr: "stun1.l.google.com:19302".to_owned(),
        turn_serv_addr: String::new(),
        username: String::new(),
        password: String::new(),
        realm: String::new(),
        software: "TEST SOFTWARE".to_owned(),
        rto_in_ms: 0,
        conn: Arc::new(conn),
        vnet: None,
    })
    .await?;

    c.listen().await?;

    Ok(c)
}

#[tokio::test]
async fn test_client_with_stun_send_binding_request() -> Result<()> {
    //env_logger::init();

    let c = create_listening_test_client_with_stun_serv().await?;

    let resp = c.send_binding_request().await?;
    log::debug!("mapped-addr: {}", resp);
    {
        let ci = c.client_internal.lock().await;
        let tm = ci.tr_map.lock().await;
        assert_eq!(0, tm.size(), "should be no transaction left");
    }

    c.close().await?;

    Ok(())
}

#[tokio::test]
async fn test_client_with_stun_send_binding_request_to_parallel() -> Result<()> {
    env_logger::init();

    let c1 = create_listening_test_client(0).await?;
    let c2 = c1.clone();

    let (stared_tx, mut started_rx) = mpsc::channel::<()>(1);
    let (finished_tx, mut finished_rx) = mpsc::channel::<()>(1);

    let to = lookup_host(true, "stun1.l.google.com:19302").await?;

    tokio::spawn(async move {
        drop(stared_tx);
        if let Ok(resp) = c2.send_binding_request_to(&to.to_string()).await {
            log::debug!("mapped-addr: {}", resp);
        }
        drop(finished_tx);
    });

    let _ = started_rx.recv().await;

    let resp = c1.send_binding_request_to(&to.to_string()).await?;
    log::debug!("mapped-addr: {}", resp);

    let _ = finished_rx.recv().await;

    c1.close().await?;

    Ok(())
}

#[tokio::test]
async fn test_client_with_stun_send_binding_request_to_timeout() -> Result<()> {
    //env_logger::init();

    let c = create_listening_test_client(10).await?;

    let to = lookup_host(true, "127.0.0.1:9").await?;

    let result = c.send_binding_request_to(&to.to_string()).await;
    assert!(result.is_err(), "expected error, but got ok");

    c.close().await?;

    Ok(())
}

struct TestAuthHandler;
impl AuthHandler for TestAuthHandler {
    fn auth_handle(&self, username: &str, realm: &str, _src_addr: SocketAddr) -> Result<Vec<u8>> {
        Ok(generate_auth_key(username, realm, "pass"))
    }
}

// Create an allocation, and then delete all nonces
// The subsequent Write on the allocation will cause a CreatePermission
// which will be forced to handle a stale nonce response
#[tokio::test]
async fn test_client_nonce_expiration() -> Result<()> {
    // env_logger::init();

    // here, it should use static port, like "0.0.0.0:3478",
    // but, due to different test environment, let's fake it by using "0.0.0.0:0"
    // to auto assign a "static" port
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

    {
        let mut nonces = server.nonces.lock().await;
        nonces.clear();
    }

    allocation
        .send_to(&[0x00], SocketAddr::from_str("127.0.0.1:8080")?)
        .await?;

    // Shutdown
    client.close().await?;
    server.close().await?;

    Ok(())
}
