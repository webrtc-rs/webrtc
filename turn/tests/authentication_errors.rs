//! Wire regressions for legacy long-term authentication failure responses.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use stun::agent::TransactionId;
use stun::attributes::*;
use stun::error_code::*;
use stun::integrity::MessageIntegrity;
use stun::message::*;
use stun::textattrs::Nonce;
use tokio::net::UdpSocket;
use tokio::time::timeout;
use turn::auth::{generate_auth_key, AuthHandler};
use turn::proto::reqtrans::RequestedTransport;
use turn::proto::PROTO_UDP;

const REALM: &str = "turn.example.org";
const PASSWORD: &str = "test-password";
const WAIT: Duration = Duration::from_secs(2);

struct FixedAuth;
impl AuthHandler for FixedAuth {
    fn auth_handle(
        &self,
        username: &str,
        realm: &str,
        _src_addr: SocketAddr,
    ) -> Result<Vec<u8>, turn::Error> {
        if username == "alice" {
            Ok(generate_auth_key(username, realm, PASSWORD))
        } else {
            Err(turn::Error::ErrNoSuchUser)
        }
    }
}

struct Server {
    inner: turn::server::Server,
    address: SocketAddr,
}

impl Server {
    async fn fixed() -> Self {
        use turn::relay::relay_static::RelayAddressGeneratorStatic;
        use turn::server::config::{ConnConfig, ServerConfig};
        use util::vnet::net::Net;
        let conn = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let address = conn.local_addr().unwrap();
        let inner = turn::server::Server::new(ServerConfig {
            conn_configs: vec![ConnConfig {
                conn,
                relay_addr_generator: Box::new(RelayAddressGeneratorStatic {
                    relay_address: "127.0.0.1".parse().unwrap(),
                    address: "127.0.0.1".into(),
                    net: Arc::new(Net::new(None)),
                }),
            }],
            realm: REALM.into(),
            auth_handler: Arc::new(FixedAuth),
            channel_bind_timeout: Duration::from_secs(600),
            alloc_close_notify: None,
        })
        .await
        .unwrap();
        Self { inner, address }
    }
    async fn client(&self) -> UdpSocket {
        let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        socket.connect(self.address).await.unwrap();
        socket
    }
    async fn close(self) {
        self.inner.close().await.unwrap();
    }
}

async fn exchange(socket: &UdpSocket, request: &Message) -> Message {
    socket.send(&request.raw).await.unwrap();
    let mut buffer = [0; 2048];
    let len = timeout(WAIT, socket.recv(&mut buffer))
        .await
        .unwrap()
        .unwrap();
    let mut response = Message::new();
    response.write(&buffer[..len]).unwrap();
    assert_eq!(response.transaction_id, request.transaction_id);
    response
}

fn request(
    method: Method,
    attrs: Vec<Box<dyn Setter>>,
    auth: Option<(&str, &str, &str, &str)>,
) -> Message {
    let mut m = Message::new();
    m.build(&[
        Box::new(TransactionId::new()),
        Box::new(MessageType::new(method, CLASS_REQUEST)),
    ])
    .unwrap();
    for attr in attrs {
        attr.add_to(&mut m).unwrap();
    }
    if let Some((user, password, realm, nonce)) = auth {
        m.add(ATTR_USERNAME, user.as_bytes());
        m.add(ATTR_REALM, realm.as_bytes());
        m.add(ATTR_NONCE, nonce.as_bytes());
        MessageIntegrity(generate_auth_key(user, realm, password))
            .add_to(&mut m)
            .unwrap();
    }
    m
}

fn transport() -> Vec<Box<dyn Setter>> {
    vec![Box::new(RequestedTransport {
        protocol: PROTO_UDP,
    })]
}

fn assert_error(response: &Message, code: ErrorCode) {
    assert_eq!(response.typ.class, CLASS_ERROR_RESPONSE);
    let mut attr = ErrorCodeAttribute::default();
    attr.get_from(response).unwrap();
    assert_eq!(attr.code.0, code.0);
}

async fn challenge(socket: &UdpSocket) -> String {
    let response = exchange(socket, &request(METHOD_ALLOCATE, transport(), None)).await;
    assert_error(&response, CODE_UNAUTHORIZED);
    let mut nonce = Nonce::new(ATTR_NONCE, String::new());
    nonce.get_from(&response).unwrap();
    assert!(!nonce.text.is_empty());
    nonce.text
}

fn assert_challenge(response: &Message, code: ErrorCode) {
    assert_error(response, code);
    assert_eq!(response.get(ATTR_REALM).unwrap(), REALM.as_bytes());
    assert!(!response.get(ATTR_NONCE).unwrap().is_empty());
    assert!(!response.contains(ATTR_MESSAGE_INTEGRITY));
}

#[tokio::test]
async fn invalid_credentials_receive_401_challenges() {
    let server = Server::fixed().await;
    let client = server.client().await;
    let nonce = challenge(&client).await;
    for (user, password) in [("alice", "wrong-password"), ("unknown-user", PASSWORD)] {
        let response = exchange(
            &client,
            &request(
                METHOD_ALLOCATE,
                transport(),
                Some((user, password, REALM, &nonce)),
            ),
        )
        .await;
        assert_challenge(&response, CODE_UNAUTHORIZED);
    }
    server.close().await;
}

#[tokio::test]
async fn invalid_integrity_precedes_stale_nonce() {
    let server = Server::fixed().await;
    let client = server.client().await;
    let response = exchange(
        &client,
        &request(
            METHOD_ALLOCATE,
            transport(),
            Some(("alice", "wrong-password", REALM, "stale")),
        ),
    )
    .await;
    assert_challenge(&response, CODE_UNAUTHORIZED);
    server.close().await;
}

#[tokio::test]
async fn missing_identity_attributes_precede_stale_nonce() {
    let server = Server::fixed().await;
    let client = server.client().await;
    for missing in [ATTR_USERNAME, ATTR_REALM, ATTR_NONCE] {
        let mut message = request(METHOD_ALLOCATE, transport(), None);
        for (typ, value) in [
            (ATTR_USERNAME, "alice"),
            (ATTR_REALM, REALM),
            (ATTR_NONCE, "stale"),
        ] {
            if typ != missing {
                message.add(typ, value.as_bytes());
            }
        }
        MessageIntegrity(generate_auth_key("alice", REALM, PASSWORD))
            .add_to(&mut message)
            .unwrap();
        let response = exchange(&client, &message).await;
        assert_error(&response, CODE_BAD_REQUEST);
    }
    server.close().await;
}

#[tokio::test]
async fn valid_integrity_with_stale_nonce_receives_a_fresh_438_challenge() {
    let server = Server::fixed().await;
    let client = server.client().await;
    let response = exchange(
        &client,
        &request(
            METHOD_ALLOCATE,
            transport(),
            Some(("alice", PASSWORD, REALM, "stale")),
        ),
    )
    .await;
    assert_challenge(&response, CODE_STALE_NONCE);
    let nonce = String::from_utf8(response.get(ATTR_NONCE).unwrap()).unwrap();
    assert_ne!(nonce, "stale");
    let accepted = exchange(
        &client,
        &request(
            METHOD_ALLOCATE,
            transport(),
            Some(("alice", PASSWORD, REALM, &nonce)),
        ),
    )
    .await;
    assert_eq!(accepted.typ.class, CLASS_SUCCESS_RESPONSE);
    MessageIntegrity(generate_auth_key("alice", REALM, PASSWORD))
        .check(&mut accepted.clone())
        .unwrap();
    server.close().await;
}
