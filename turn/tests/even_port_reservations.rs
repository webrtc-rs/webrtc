//! Wire regressions for RFC 8656 section 7.2 EVEN-PORT reservations.

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
use turn::proto::relayaddr::RelayedAddress;
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
        Ok(generate_auth_key(username, realm, PASSWORD))
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

fn assert_integrity(response: &Message, user: &str, password: &str) {
    MessageIntegrity(generate_auth_key(user, REALM, password))
        .check(&mut response.clone())
        .unwrap();
}

async fn challenge(socket: &UdpSocket) -> String {
    let response = exchange(socket, &request(METHOD_ALLOCATE, transport(), None)).await;
    assert_error(&response, CODE_UNAUTHORIZED);
    let mut nonce = Nonce::new(ATTR_NONCE, String::new());
    nonce.get_from(&response).unwrap();
    assert!(!nonce.text.is_empty());
    nonce.text
}

struct Raw(AttrType, Vec<u8>);
impl Setter for Raw {
    fn add_to(&self, m: &mut Message) -> Result<(), stun::Error> {
        m.add(self.0, &self.1);
        Ok(())
    }
}

#[tokio::test]
async fn even_port_reservation_is_server_wide_across_listeners() {
    use turn::relay::relay_range::RelayAddressGeneratorRanges;
    use turn::server::config::{ConnConfig, ServerConfig};
    use util::vnet::net::Net;
    let first = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let second = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let first_addr = first.local_addr().unwrap();
    let second_addr = second.local_addr().unwrap();
    let generator = |min_port, max_port| {
        Box::new(RelayAddressGeneratorRanges {
            relay_address: "127.0.0.1".parse().unwrap(),
            min_port,
            max_port,
            max_retries: 100,
            address: "127.0.0.1".into(),
            net: Arc::new(Net::new(None)),
        })
    };
    let server = turn::server::Server::new(ServerConfig {
        conn_configs: vec![
            ConnConfig {
                conn: first,
                relay_addr_generator: generator(49152, 57343),
            },
            ConnConfig {
                conn: second,
                relay_addr_generator: generator(57344, 65535),
            },
        ],
        realm: REALM.into(),
        auth_handler: Arc::new(FixedAuth),
        channel_bind_timeout: Duration::from_secs(600),
        alloc_close_notify: None,
    })
    .await
    .unwrap();
    let a = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    a.connect(first_addr).await.unwrap();
    let nonce_a = challenge(&a).await;
    let mut attrs = transport();
    attrs.push(Box::new(Raw(ATTR_EVEN_PORT, vec![0x80])));
    let response = exchange(
        &a,
        &request(
            METHOD_ALLOCATE,
            attrs,
            Some(("alice", PASSWORD, REALM, &nonce_a)),
        ),
    )
    .await;
    assert_eq!(response.typ.class, CLASS_SUCCESS_RESPONSE);
    assert_integrity(&response, "alice", PASSWORD);
    let token = response.get(ATTR_RESERVATION_TOKEN).unwrap();
    let mut even_relay = RelayedAddress::default();
    even_relay.get_from(&response).unwrap();
    let odd_address = SocketAddr::new(even_relay.ip, even_relay.port + 1);
    assert!(UdpSocket::bind(odd_address).await.is_err());
    let b = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    b.connect(second_addr).await.unwrap();
    let nonce_b = challenge(&b).await;
    let mut attrs = transport();
    attrs.push(Box::new(Raw(ATTR_RESERVATION_TOKEN, token.clone())));
    let response = exchange(
        &b,
        &request(
            METHOD_ALLOCATE,
            attrs,
            Some(("alice", PASSWORD, REALM, &nonce_b)),
        ),
    )
    .await;
    assert_eq!(
        response.typ.class, CLASS_SUCCESS_RESPONSE,
        "RFC 8656 section 7.2 permits consuming a reservation on another server listener"
    );
    assert_integrity(&response, "alice", PASSWORD);
    let mut odd_relay = RelayedAddress::default();
    odd_relay.get_from(&response).unwrap();
    assert_eq!(SocketAddr::new(odd_relay.ip, odd_relay.port), odd_address);
    assert!(UdpSocket::bind(odd_address).await.is_err());

    // A different five-tuple cannot consume the token a second time.
    let c = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    c.connect(first_addr).await.unwrap();
    let nonce_c = challenge(&c).await;
    let mut attrs = transport();
    attrs.push(Box::new(Raw(ATTR_RESERVATION_TOKEN, token)));
    let reused = exchange(
        &c,
        &request(
            METHOD_ALLOCATE,
            attrs,
            Some(("alice", PASSWORD, REALM, &nonce_c)),
        ),
    )
    .await;
    assert_error(&reused, CODE_INSUFFICIENT_CAPACITY);
    assert_integrity(&reused, "alice", PASSWORD);
    server.close().await.unwrap();
    // Shutdown must release both allocations, including the transferred socket.
    let released = UdpSocket::bind(odd_address).await.unwrap();
    drop(released);
}

#[tokio::test]
async fn even_port_reservation_lifetime_and_shutdown_release_sockets() {
    for expire in [true, false] {
        let server = Server::fixed().await;
        let client = server.client().await;
        let nonce = challenge(&client).await;
        let mut attrs = transport();
        attrs.push(Box::new(Raw(ATTR_EVEN_PORT, vec![0x80])));
        let allocated = exchange(
            &client,
            &request(
                METHOD_ALLOCATE,
                attrs,
                Some(("alice", PASSWORD, REALM, &nonce)),
            ),
        )
        .await;
        assert_eq!(allocated.typ.class, CLASS_SUCCESS_RESPONSE);
        let token = allocated.get(ATTR_RESERVATION_TOKEN).unwrap();
        let mut relay = RelayedAddress::default();
        relay.get_from(&allocated).unwrap();
        let odd_address = SocketAddr::new(relay.ip, relay.port + 1);
        assert!(UdpSocket::bind(odd_address).await.is_err());
        if expire {
            tokio::time::sleep(Duration::from_secs(29)).await;
            assert!(UdpSocket::bind(odd_address).await.is_err());
            tokio::time::sleep(Duration::from_secs(2)).await;
            tokio::task::yield_now().await;
            let other = server.client().await;
            let nonce = challenge(&other).await;
            let mut attrs = transport();
            attrs.push(Box::new(Raw(ATTR_RESERVATION_TOKEN, token)));
            let expired = exchange(
                &other,
                &request(
                    METHOD_ALLOCATE,
                    attrs,
                    Some(("alice", PASSWORD, REALM, &nonce)),
                ),
            )
            .await;
            assert_error(&expired, CODE_INSUFFICIENT_CAPACITY);
            assert_integrity(&expired, "alice", PASSWORD);
            let released = UdpSocket::bind(odd_address).await.unwrap();
            drop(released);
        }
        server.close().await;
        let released = UdpSocket::bind(odd_address).await.unwrap();
        drop(released);
    }
}

#[tokio::test]
async fn even_port_r0_does_not_reserve_or_return_a_token() {
    let server = Server::fixed().await;
    let client = server.client().await;
    let nonce = challenge(&client).await;
    let mut attrs = transport();
    attrs.push(Box::new(Raw(ATTR_EVEN_PORT, vec![0])));
    let allocated = exchange(
        &client,
        &request(
            METHOD_ALLOCATE,
            attrs,
            Some(("alice", PASSWORD, REALM, &nonce)),
        ),
    )
    .await;
    assert_eq!(allocated.typ.class, CLASS_SUCCESS_RESPONSE);
    assert!(!allocated.contains(ATTR_RESERVATION_TOKEN));
    assert_integrity(&allocated, "alice", PASSWORD);
    let mut relay = RelayedAddress::default();
    relay.get_from(&allocated).unwrap();
    assert_eq!(relay.port % 2, 0);
    let odd = UdpSocket::bind(SocketAddr::new(relay.ip, relay.port + 1))
        .await
        .unwrap();
    drop(odd);
    server.close().await;
}
