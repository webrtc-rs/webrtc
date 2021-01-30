use super::config::*;
use super::*;
use crate::auth::generate_auth_key;
use crate::client::*;
use crate::errors::*;
use crate::relay::relay_static::*;

use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use tokio::net::UdpSocket;
use util::Error;

struct TestAuthHandler {
    cred_map: HashMap<String, Vec<u8>>,
}

impl TestAuthHandler {
    fn new() -> Self {
        let mut cred_map = HashMap::new();
        cred_map.insert(
            "user".to_owned(),
            generate_auth_key("user", "webrtc.rs", "pass"),
        );

        TestAuthHandler { cred_map }
    }
}

impl AuthHandler for TestAuthHandler {
    fn auth_handle(
        &self,
        username: &str,
        _realm: &str,
        _src_addr: SocketAddr,
    ) -> Result<Vec<u8>, Error> {
        if let Some(pw) = self.cred_map.get(username) {
            Ok(pw.to_vec())
        } else {
            Err(ERR_FAKE_ERR.to_owned())
        }
    }
}

#[tokio::test]
async fn test_server_simple() -> Result<(), Error> {
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
            }),
        }],
        realm: "webrtc.rs".to_owned(),
        auth_handler: Arc::new(Box::new(TestAuthHandler::new())),
        channel_bind_timeout: Duration::from_secs(0),
    })
    .await?;

    assert_eq!(
        DEFAULT_LIFETIME, server.channel_bind_timeout,
        "should match"
    );

    let conn = Arc::new(UdpSocket::bind("0.0.0.0:0").await?);

    let client = Client::new(ClientConfig {
        stun_serv_addr: String::new(),
        turn_serv_addr: String::new(),
        username: String::new(),
        password: String::new(),
        realm: String::new(),
        software: String::new(),
        rto_in_ms: 0,
        conn,
    })
    .await?;

    client.listen().await?;

    client
        .send_binding_request_to(format!("127.0.0.1:{}", server_port).as_str())
        .await?;

    client.close().await?;
    server.close()?;

    Ok(())
}
