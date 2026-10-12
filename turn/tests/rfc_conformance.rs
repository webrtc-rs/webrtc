//! UDP wire tests independent of any application identity or signaling service.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::prelude::*;
use hmac::{Hmac, Mac};
use sha1::Sha1;
use stun::agent::TransactionId;
use stun::attributes::*;
use stun::error_code::*;
use stun::fingerprint::FINGERPRINT;
use stun::integrity::MessageIntegrity;
use stun::message::*;
use stun::textattrs::{Nonce, Realm};
use stun::uattrs::UnknownAttributes;
use stun::xoraddr::XorMappedAddress;
use tokio::net::UdpSocket;
use tokio::time::timeout;
use turn::auth::AuthHandler;
use turn::proto::chandata::ChannelData;
use turn::proto::channum::ChannelNumber;
use turn::proto::lifetime::Lifetime;
use turn::proto::peeraddr::PeerAddress;
use turn::proto::relayaddr::RelayedAddress;
use turn::proto::reqtrans::RequestedTransport;
use turn::proto::PROTO_UDP;

const REALM: &str = "turn.example.org";
const PASSWORD: &str = "test-password";
const WAIT: Duration = Duration::from_secs(2);

async fn create_turn_server(
    socket: Arc<UdpSocket>,
    advertised_ip: &str,
    realm: &str,
    auth_handler: Arc<dyn AuthHandler + Send + Sync>,
) -> Result<turn::server::Server, turn::Error> {
    create_turn_server_with_port_range(socket, advertised_ip, realm, auth_handler, (49152, 65535))
        .await
}

async fn create_turn_server_with_port_range(
    socket: Arc<UdpSocket>,
    advertised_ip: &str,
    realm: &str,
    auth_handler: Arc<dyn AuthHandler + Send + Sync>,
    ports: (u16, u16),
) -> Result<turn::server::Server, turn::Error> {
    use turn::relay::relay_range::RelayAddressGeneratorRanges;
    use turn::server::config::{ConnConfig, ServerConfig};
    use util::vnet::net::Net;
    turn::server::Server::new(ServerConfig {
        conn_configs: vec![ConnConfig {
            relay_addr_generator: Box::new(RelayAddressGeneratorRanges {
                relay_address: advertised_ip.parse().unwrap(),
                min_port: ports.0,
                max_port: ports.1,
                max_retries: 100,
                address: socket.local_addr()?.ip().to_string(),
                net: Arc::new(Net::new(None)),
            }),
            conn: socket,
        }],
        realm: realm.into(),
        auth_handler,
        channel_bind_timeout: Duration::from_secs(600),
        alloc_close_notify: None,
    })
    .await
}

struct Server {
    inner: turn::server::Server,
    address: SocketAddr,
}

impl Server {
    async fn start(auth: Arc<dyn AuthHandler + Send + Sync>) -> Self {
        let socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let address = socket.local_addr().unwrap();
        let inner = create_turn_server(socket, "127.0.0.1", REALM, auth)
            .await
            .unwrap();
        Self { inner, address }
    }

    async fn fixed() -> Self {
        Self::start(Arc::new(
            turn::auth::StaticAuthHandler::new(BTreeMap::from([
                ("alice".into(), PASSWORD.into()),
                ("bob".into(), PASSWORD.into()),
            ]))
            .unwrap(),
        ))
        .await
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
        MessageIntegrity(turn::auth::generate_auth_key(user, realm, password))
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
    MessageIntegrity(turn::auth::generate_auth_key(user, REALM, password))
        .check(&mut response.clone())
        .unwrap();
}

async fn challenge(socket: &UdpSocket) -> String {
    let response = exchange(socket, &request(METHOD_ALLOCATE, transport(), None)).await;
    assert_error(&response, CODE_UNAUTHORIZED);
    let mut realm = Realm::new(ATTR_REALM, String::new());
    realm.get_from(&response).unwrap();
    assert_eq!(realm.text, REALM);
    let mut nonce = Nonce::new(ATTR_NONCE, String::new());
    nonce.get_from(&response).unwrap();
    assert!(!nonce.text.is_empty());
    nonce.text
}

#[tokio::test]
async fn binding_and_long_term_authentication_errors() {
    let server = Server::fixed().await;
    let client = server.client().await;
    let response = exchange(&client, &request(METHOD_BINDING, vec![], None)).await;
    assert_eq!(response.typ, BINDING_SUCCESS);
    let mut mapped = XorMappedAddress::default();
    mapped.get_from(&response).unwrap();
    assert_eq!(
        SocketAddr::new(mapped.ip, mapped.port),
        client.local_addr().unwrap()
    );
    let nonce = challenge(&client).await;
    for (user, password, realm, n, code) in [
        ("alice", "wrong", REALM, nonce.as_str(), CODE_UNAUTHORIZED),
        (
            "unknown",
            PASSWORD,
            REALM,
            nonce.as_str(),
            CODE_UNAUTHORIZED,
        ),
        (
            "alice",
            PASSWORD,
            "other.example",
            nonce.as_str(),
            CODE_UNAUTHORIZED,
        ),
        ("alice", PASSWORD, REALM, "stale", CODE_STALE_NONCE),
        ("alice", "wrong", REALM, "stale", CODE_UNAUTHORIZED),
    ] {
        let response = exchange(
            &client,
            &request(
                METHOD_ALLOCATE,
                transport(),
                Some((user, password, realm, n)),
            ),
        )
        .await;
        assert_error(&response, code);
        assert!(response.contains(ATTR_REALM));
        assert!(response.contains(ATTR_NONCE));
        assert!(!response.contains(ATTR_MESSAGE_INTEGRITY));
    }
    server.close().await;
}

#[tokio::test]
async fn allocation_retransmission_ownership_and_missing_allocation() {
    let server = Server::fixed().await;
    let client = server.client().await;
    let nonce = challenge(&client).await;
    let auth = Some(("alice", PASSWORD, REALM, nonce.as_str()));
    let allocate = request(METHOD_ALLOCATE, transport(), auth);
    let first = exchange(&client, &allocate).await;
    assert_eq!(first.typ.class, CLASS_SUCCESS_RESPONSE);
    assert_integrity(&first, "alice", PASSWORD);
    let duplicate = exchange(&client, &allocate).await;
    assert_eq!(first.raw, duplicate.raw);
    let different = exchange(&client, &request(METHOD_ALLOCATE, transport(), auth)).await;
    assert_error(&different, CODE_ALLOC_MISMATCH);
    for method in [
        METHOD_REFRESH,
        METHOD_CREATE_PERMISSION,
        METHOD_CHANNEL_BIND,
    ] {
        let response = exchange(
            &client,
            &request(method, vec![], Some(("bob", PASSWORD, REALM, &nonce))),
        )
        .await;
        assert_error(&response, CODE_WRONG_CREDENTIALS);
        assert_integrity(&response, "bob", PASSWORD);
    }
    let deleted = exchange(
        &client,
        &request(
            METHOD_REFRESH,
            vec![Box::new(Lifetime(Duration::ZERO))],
            auth,
        ),
    )
    .await;
    assert_eq!(deleted.typ.class, CLASS_SUCCESS_RESPONSE);
    for method in [
        METHOD_REFRESH,
        METHOD_CREATE_PERMISSION,
        METHOD_CHANNEL_BIND,
    ] {
        let response = exchange(&client, &request(method, vec![], auth)).await;
        assert_error(&response, CODE_ALLOC_MISMATCH);
        assert_integrity(&response, "alice", PASSWORD);
    }
    server.close().await;
}

#[tokio::test]
async fn cached_allocation_keys_survive_admission_credential_expiry() {
    let secret = "test-shared-secret";
    let server = Server::start(Arc::new(turn::auth::LongTermAuthHandler::new(
        secret.into(),
    )))
    .await;
    let client = server.client().await;
    let expires = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 2;
    // Timestamp-only usernames are valid; no application-specific suffix is needed.
    let user = expires.to_string();
    let mut mac = Hmac::<Sha1>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(user.as_bytes());
    let password = BASE64_STANDARD.encode(mac.finalize().into_bytes());
    let nonce = challenge(&client).await;
    let auth = Some((user.as_str(), password.as_str(), REALM, nonce.as_str()));
    let allocated = exchange(&client, &request(METHOD_ALLOCATE, transport(), auth)).await;
    assert_eq!(allocated.typ.class, CLASS_SUCCESS_RESPONSE);
    let mut relay = RelayedAddress::default();
    relay.get_from(&allocated).unwrap();
    let peer = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let peer_addr = peer.local_addr().unwrap();
    let until_expiry =
        Duration::from_secs(expires) - SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    tokio::time::sleep(until_expiry + Duration::from_millis(1100)).await;
    let response = exchange(&client, &request(METHOD_REFRESH, vec![], auth)).await;
    assert_eq!(response.typ.class, CLASS_SUCCESS_RESPONSE);
    assert_integrity(&response, &user, &password);
    let key =
        <sha2::Sha256 as sha2::Digest>::digest(format!("{user}:{REALM}:{password}").as_bytes())
            .to_vec();
    let mut modern_refresh = request(METHOD_REFRESH, vec![], None);
    modern_refresh.add(ATTR_USERNAME, user.as_bytes());
    modern_refresh.add(ATTR_REALM, REALM.as_bytes());
    modern_refresh.add(ATTR_NONCE, nonce.as_bytes());
    modern_refresh.add(ATTR_PASSWORD_ALGORITHMS, &[0, 2, 0, 0, 0, 1, 0, 0]);
    modern_refresh.add(ATTR_PASSWORD_ALGORITHM, &[0, 2, 0, 0]);
    sign_sha256(&mut modern_refresh, &key);
    let response = exchange(&client, &modern_refresh).await;
    assert_eq!(response.typ.class, CLASS_SUCCESS_RESPONSE);
    verify_sha256(&response, &key);
    let response = exchange(
        &client,
        &request(
            METHOD_CREATE_PERMISSION,
            vec![Box::new(PeerAddress {
                ip: peer_addr.ip(),
                port: peer_addr.port(),
            })],
            auth,
        ),
    )
    .await;
    assert_eq!(response.typ.class, CLASS_SUCCESS_RESPONSE);
    let response = exchange(
        &client,
        &request(
            METHOD_CHANNEL_BIND,
            vec![
                Box::new(ChannelNumber(0x4000)),
                Box::new(PeerAddress {
                    ip: peer_addr.ip(),
                    port: peer_addr.port(),
                }),
            ],
            auth,
        ),
    )
    .await;
    assert_eq!(response.typ.class, CLASS_SUCCESS_RESPONSE);
    peer.send_to(b"relay-still-works", SocketAddr::new(relay.ip, relay.port))
        .await
        .unwrap();
    let mut buffer = [0; 2048];
    let len = timeout(WAIT, client.recv(&mut buffer))
        .await
        .unwrap()
        .unwrap();
    let mut data = ChannelData {
        raw: buffer[..len].to_vec(),
        ..Default::default()
    };
    data.decode().unwrap();
    assert_eq!(data.data, b"relay-still-works");
    let new_client = server.client().await;
    let response = exchange(&new_client, &request(METHOD_ALLOCATE, transport(), auth)).await;
    assert_error(&response, CODE_UNAUTHORIZED);
    server.close().await;
}

#[tokio::test]
async fn unknown_required_attributes_and_untrusted_integrity_suffix() {
    let server = Server::fixed().await;
    let client = server.client().await;
    let mut binding = request(METHOD_BINDING, vec![], None);
    binding.add(AttrType(0x1234), &[0, 0, 0, 0]);
    let response = exchange(&client, &binding).await;
    assert_error(&response, CODE_UNKNOWN_ATTRIBUTE);
    let mut unknown = UnknownAttributes(vec![]);
    unknown.get_from(&response).unwrap();
    assert_eq!(unknown.0, vec![AttrType(0x1234)]);
    let mut binding = request(METHOD_BINDING, vec![], None);
    binding.add(AttrType(0x9234), &[0, 0, 0, 0]);
    FINGERPRINT.add_to(&mut binding).unwrap();
    assert_eq!(exchange(&client, &binding).await.typ, BINDING_SUCCESS);
    let nonce = challenge(&client).await;
    let auth = Some(("alice", PASSWORD, REALM, nonce.as_str()));
    let allocated = exchange(&client, &request(METHOD_ALLOCATE, transport(), auth)).await;
    assert_eq!(allocated.typ.class, CLASS_SUCCESS_RESPONSE);
    let mut refresh = request(METHOD_REFRESH, vec![], auth);
    // An unsigned suffix must not delete the allocation.
    Lifetime(Duration::ZERO).add_to(&mut refresh).unwrap();
    FINGERPRINT.add_to(&mut refresh).unwrap();
    let response = exchange(&client, &refresh).await;
    assert_eq!(response.typ.class, CLASS_SUCCESS_RESPONSE);
    let mut lifetime = Lifetime::default();
    lifetime.get_from(&response).unwrap();
    assert!(lifetime.0 > Duration::ZERO);
    let response = exchange(&client, &request(METHOD_REFRESH, vec![], auth)).await;
    assert_eq!(response.typ.class, CLASS_SUCCESS_RESPONSE);
    server.close().await;
}

struct Raw(AttrType, Vec<u8>);
impl Setter for Raw {
    fn add_to(&self, m: &mut Message) -> Result<(), stun::Error> {
        m.add(self.0, &self.1);
        Ok(())
    }
}

#[tokio::test]
async fn authenticated_validation_and_even_port_reservations() {
    let server = Server::fixed().await;
    let client = server.client().await;
    let nonce = challenge(&client).await;
    let auth = Some(("alice", PASSWORD, REALM, nonce.as_str()));
    let missing = exchange(&client, &request(METHOD_ALLOCATE, vec![], auth)).await;
    assert_error(&missing, CODE_BAD_REQUEST);
    assert_integrity(&missing, "alice", PASSWORD);
    let cases: Vec<(Vec<Box<dyn Setter>>, ErrorCode)> = vec![
        (
            vec![Box::new(RequestedTransport {
                protocol: turn::proto::Protocol(6),
            })],
            CODE_UNSUPPORTED_TRANS_PROTO,
        ),
        (
            vec![Box::new(Raw(ATTR_DONT_FRAGMENT, vec![]))],
            CODE_UNKNOWN_ATTRIBUTE,
        ),
        (
            vec![Box::new(Raw(ATTR_LIFETIME, vec![0]))],
            CODE_BAD_REQUEST,
        ),
        (
            vec![Box::new(Raw(
                ATTR_REQUESTED_ADDRESS_FAMILY,
                vec![2, 0, 0, 0],
            ))],
            CODE_ADDR_FAMILY_NOT_SUPPORTED,
        ),
        (
            vec![Box::new(Raw(ATTR_RESERVATION_TOKEN, vec![0xff; 8]))],
            CODE_INSUFFICIENT_CAPACITY,
        ),
        (
            vec![Box::new(Raw(ATTR_EVEN_PORT, vec![0; 2]))],
            CODE_BAD_REQUEST,
        ),
        (
            vec![Box::new(Raw(AttrType(0x8000), vec![1, 0, 0, 0]))],
            CODE_BAD_REQUEST,
        ),
    ];
    for (mut attrs, code) in cases {
        attrs.extend(transport());
        let response = exchange(&client, &request(METHOD_ALLOCATE, attrs, auth)).await;
        assert_error(&response, code);
        assert_integrity(&response, "alice", PASSWORD);
    }
    let mut attrs = transport();
    attrs.push(Box::new(Raw(ATTR_EVEN_PORT, vec![0x80])));
    let allocated = exchange(&client, &request(METHOD_ALLOCATE, attrs, auth)).await;
    assert_eq!(allocated.typ.class, CLASS_SUCCESS_RESPONSE);
    let token = allocated.get(ATTR_RESERVATION_TOKEN).unwrap();
    assert_eq!(token.len(), 8);
    let mut relay = RelayedAddress::default();
    relay.get_from(&allocated).unwrap();
    assert_eq!(relay.port % 2, 0);
    // The odd socket must already be bound, not merely recorded in a map.
    assert!(UdpSocket::bind(SocketAddr::new(relay.ip, relay.port + 1))
        .await
        .is_err());
    let second = server.client().await;
    let mut attrs = transport();
    attrs.push(Box::new(Raw(ATTR_RESERVATION_TOKEN, token.clone())));
    let reserved = exchange(&second, &request(METHOD_ALLOCATE, attrs, auth)).await;
    assert_eq!(reserved.typ.class, CLASS_SUCCESS_RESPONSE);
    let mut odd = RelayedAddress::default();
    odd.get_from(&reserved).unwrap();
    assert_eq!(odd.port, relay.port + 1);
    let third = server.client().await;
    let mut attrs = transport();
    attrs.push(Box::new(Raw(ATTR_RESERVATION_TOKEN, token)));
    assert_error(
        &exchange(&third, &request(METHOD_ALLOCATE, attrs, auth)).await,
        CODE_INSUFFICIENT_CAPACITY,
    );
    for number in [0x3fff, 0x5000, 0x7fff] {
        let response = exchange(
            &client,
            &request(
                METHOD_CHANNEL_BIND,
                vec![
                    Box::new(ChannelNumber(number)),
                    Box::new(PeerAddress {
                        ip: "127.0.0.1".parse().unwrap(),
                        port: 9999,
                    }),
                ],
                auth,
            ),
        )
        .await;
        assert_error(&response, CODE_BAD_REQUEST);
        assert_integrity(&response, "alice", PASSWORD);
    }
    let response = exchange(
        &client,
        &request(
            METHOD_REFRESH,
            vec![
                Box::new(Raw(ATTR_REQUESTED_ADDRESS_FAMILY, vec![2, 0, 0, 0])),
                Box::new(Lifetime(Duration::ZERO)),
            ],
            auth,
        ),
    )
    .await;
    assert_error(&response, CODE_PEER_ADDR_FAMILY_MISMATCH);
    assert_integrity(&response, "alice", PASSWORD);
    server.close().await;
}

#[tokio::test]
async fn multiple_permissions_large_datagrams_and_partial_dual_stack() {
    let server = Server::fixed().await;
    let client = server.client().await;
    let nonce = challenge(&client).await;
    let auth = Some(("alice", PASSWORD, REALM, nonce.as_str()));
    let mut attrs = transport();
    attrs.push(Box::new(Raw(AttrType(0x8000), vec![2, 0, 0, 0])));
    let allocated = exchange(&client, &request(METHOD_ALLOCATE, attrs, auth)).await;
    assert_eq!(allocated.typ.class, CLASS_SUCCESS_RESPONSE);
    assert_eq!(
        &allocated.get(AttrType(0x8001)).unwrap()[..4],
        &[2, 0, 4, 40]
    );
    let mut relay = RelayedAddress::default();
    relay.get_from(&allocated).unwrap();
    let peer1 = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let peer2 = UdpSocket::bind("127.0.0.2:0").await.unwrap();
    let mut peers: Vec<Box<dyn Setter>> = Vec::new();
    for peer in [&peer1, &peer2] {
        let address = peer.local_addr().unwrap();
        peers.push(Box::new(PeerAddress {
            ip: address.ip(),
            port: address.port(),
        }));
    }
    let response = exchange(&client, &request(METHOD_CREATE_PERMISSION, peers, auth)).await;
    assert_eq!(response.typ.class, CLASS_SUCCESS_RESPONSE);
    let payload = vec![0xa5; 8192];
    let mut buffer = vec![0; 65535];
    for peer in [&peer1, &peer2] {
        peer.send_to(&payload, SocketAddr::new(relay.ip, relay.port))
            .await
            .unwrap();
        let len = timeout(WAIT, client.recv(&mut buffer))
            .await
            .unwrap()
            .unwrap();
        let mut data = Message::new();
        data.write(&buffer[..len]).unwrap();
        assert_eq!(data.typ, MessageType::new(METHOD_DATA, CLASS_INDICATION));
        assert_eq!(data.get(ATTR_DATA).unwrap(), payload);
        let mut address = PeerAddress::default();
        address.get_from(&data).unwrap();
        assert_eq!(
            SocketAddr::new(address.ip, address.port),
            peer.local_addr().unwrap()
        );
    }
    let peer = peer1.local_addr().unwrap();
    let response = exchange(
        &client,
        &request(
            METHOD_CHANNEL_BIND,
            vec![
                Box::new(ChannelNumber(0x4fff)),
                Box::new(PeerAddress {
                    ip: peer.ip(),
                    port: peer.port(),
                }),
            ],
            auth,
        ),
    )
    .await;
    assert_eq!(response.typ.class, CLASS_SUCCESS_RESPONSE);
    let mut data = ChannelData {
        number: ChannelNumber(0x4fff),
        data: payload.clone(),
        ..Default::default()
    };
    data.encode();
    client.send(&data.raw).await.unwrap();
    let (len, _) = timeout(WAIT, peer1.recv_from(&mut buffer))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(buffer[..len], payload);
    server.close().await;
}

fn sign_sha256(message: &mut Message, key: &[u8]) {
    let mut input = message.raw.clone();
    input[2..4].copy_from_slice(&((message.length + 36) as u16).to_be_bytes());
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(key).unwrap();
    mac.update(&input);
    message.add(ATTR_MESSAGE_INTEGRITY_SHA256, &mac.finalize().into_bytes());
}

fn verify_sha256(message: &Message, key: &[u8]) {
    let value = message.get(ATTR_MESSAGE_INTEGRITY_SHA256).unwrap();
    let offset = message.raw.len() - value.len() - 4;
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(key).unwrap();
    mac.update(&message.raw[..offset]);
    mac.verify_slice(&value).unwrap();
}

#[tokio::test]
async fn rfc8489_nonce_password_negotiation_and_sha256_integrity() {
    let server = Server::fixed().await;
    let client = server.client().await;
    let challenge = exchange(&client, &request(METHOD_ALLOCATE, transport(), None)).await;
    let nonce = String::from_utf8(challenge.get(ATTR_NONCE).unwrap()).unwrap();
    assert!(nonce.starts_with("obMatJos2gAAA"));
    assert_eq!(
        challenge.get(ATTR_PASSWORD_ALGORITHMS).unwrap(),
        [0, 2, 0, 0, 0, 1, 0, 0]
    );
    let key =
        <sha2::Sha256 as sha2::Digest>::digest(format!("alice:{REALM}:{PASSWORD}").as_bytes())
            .to_vec();
    let mut allocate = request(METHOD_ALLOCATE, transport(), None);
    allocate.add(ATTR_USERNAME, b"alice");
    allocate.add(ATTR_REALM, REALM.as_bytes());
    allocate.add(ATTR_NONCE, nonce.as_bytes());
    allocate.add(ATTR_PASSWORD_ALGORITHMS, &[0, 2, 0, 0, 0, 1, 0, 0]);
    allocate.add(ATTR_PASSWORD_ALGORITHM, &[0, 2, 0, 0]);
    sign_sha256(&mut allocate, &key);
    let response = exchange(&client, &allocate).await;
    assert_eq!(response.typ.class, CLASS_SUCCESS_RESPONSE);
    verify_sha256(&response, &key);
    let mut refresh = request(
        METHOD_REFRESH,
        vec![Box::new(Lifetime(Duration::ZERO))],
        None,
    );
    refresh.add(ATTR_USERNAME, b"alice");
    refresh.add(ATTR_REALM, REALM.as_bytes());
    refresh.add(ATTR_NONCE, nonce.as_bytes());
    refresh.add(ATTR_PASSWORD_ALGORITHMS, &[0, 2, 0, 0, 0, 1, 0, 0]);
    refresh.add(ATTR_PASSWORD_ALGORITHM, &[0, 2, 0, 0]);
    sign_sha256(&mut refresh, &key);
    let released = exchange(&client, &refresh).await;
    assert_eq!(released.typ.class, CLASS_SUCCESS_RESPONSE);
    verify_sha256(&released, &key);
    let mut invalid = request(METHOD_REFRESH, vec![], None);
    invalid.add(ATTR_USERNAME, b"alice");
    invalid.add(ATTR_REALM, REALM.as_bytes());
    invalid.add(ATTR_NONCE, nonce.as_bytes());
    invalid.add(ATTR_PASSWORD_ALGORITHM, &[0, 2, 0, 0]);
    sign_sha256(&mut invalid, &key);
    assert_error(&exchange(&client, &invalid).await, CODE_BAD_REQUEST);
    server.close().await;
}

#[tokio::test]
async fn dual_integrity_prefers_sha256_and_discards_unsigned_attributes() {
    let server = Server::fixed().await;
    let client = server.client().await;
    let nonce = challenge(&client).await;
    let auth = Some(("alice", PASSWORD, REALM, nonce.as_str()));
    let allocated = exchange(&client, &request(METHOD_ALLOCATE, transport(), auth)).await;
    assert_eq!(allocated.typ.class, CLASS_SUCCESS_RESPONSE);
    let key = turn::auth::generate_auth_key("alice", REALM, PASSWORD);
    let mut refresh = request(
        METHOD_REFRESH,
        vec![
            Box::new(Raw(ATTR_PASSWORD_ALGORITHMS, vec![0, 2, 0, 0, 0, 1, 0, 0])),
            Box::new(Raw(ATTR_PASSWORD_ALGORITHM, vec![0, 1, 0, 0])),
        ],
        auth,
    );
    Lifetime(Duration::ZERO).add_to(&mut refresh).unwrap();
    sign_sha256(&mut refresh, &key);
    let response = exchange(&client, &refresh).await;
    assert_eq!(response.typ.class, CLASS_SUCCESS_RESPONSE);
    verify_sha256(&response, &key);
    let mut lifetime = Lifetime::default();
    lifetime.get_from(&response).unwrap();
    assert_eq!(lifetime.0, Duration::from_secs(600));
    let last = refresh.raw.len() - 1;
    refresh.raw[last] ^= 1;
    assert_error(&exchange(&client, &refresh).await, CODE_UNAUTHORIZED);
    server.close().await;
}

#[tokio::test]
async fn ipv6_relay_and_forbidden_tunnel_addresses() {
    let socket = Arc::new(UdpSocket::bind("[::1]:0").await.unwrap());
    let address = socket.local_addr().unwrap();
    let auth = Arc::new(
        turn::auth::StaticAuthHandler::new(BTreeMap::from([("alice".into(), PASSWORD.into())]))
            .unwrap(),
    );
    let server = create_turn_server(socket, "::1", REALM, auth)
        .await
        .unwrap();
    let client = UdpSocket::bind("[::1]:0").await.unwrap();
    client.connect(address).await.unwrap();
    let nonce = challenge(&client).await;
    let auth = Some(("alice", PASSWORD, REALM, nonce.as_str()));
    let mut attrs = transport();
    attrs.push(Box::new(Raw(
        ATTR_REQUESTED_ADDRESS_FAMILY,
        vec![2, 0, 0, 0],
    )));
    let allocated = exchange(&client, &request(METHOD_ALLOCATE, attrs, auth)).await;
    assert_eq!(allocated.typ.class, CLASS_SUCCESS_RESPONSE);
    let mut relay = RelayedAddress::default();
    relay.get_from(&allocated).unwrap();
    assert!(relay.ip.is_ipv6());
    let peer = UdpSocket::bind("[::1]:0").await.unwrap();
    let address = peer.local_addr().unwrap();
    let response = exchange(
        &client,
        &request(
            METHOD_CHANNEL_BIND,
            vec![
                Box::new(ChannelNumber(0x4000)),
                Box::new(PeerAddress {
                    ip: address.ip(),
                    port: address.port(),
                }),
            ],
            auth,
        ),
    )
    .await;
    assert_eq!(response.typ.class, CLASS_SUCCESS_RESPONSE);
    peer.send_to(b"ipv6-relay", SocketAddr::new(relay.ip, relay.port))
        .await
        .unwrap();
    let mut buffer = [0; 2048];
    let len = timeout(WAIT, client.recv(&mut buffer))
        .await
        .unwrap()
        .unwrap();
    let mut data = ChannelData {
        raw: buffer[..len].to_vec(),
        ..Default::default()
    };
    data.decode().unwrap();
    assert_eq!(data.data, b"ipv6-relay");
    for ip in ["2002:c000:201::1", "2001:0:1234::1"] {
        for method in [METHOD_CHANNEL_BIND, METHOD_CREATE_PERMISSION] {
            let mut attrs: Vec<Box<dyn Setter>> = vec![Box::new(PeerAddress {
                ip: ip.parse().unwrap(),
                port: 9000,
            })];
            if method == METHOD_CHANNEL_BIND {
                attrs.push(Box::new(ChannelNumber(0x4001)));
            }
            let response = exchange(&client, &request(method, attrs, auth)).await;
            assert_error(&response, CODE_FORBIDDEN);
            assert_integrity(&response, "alice", PASSWORD);
        }
    }
    server.close().await.unwrap();
}

#[tokio::test]
async fn malformed_fingerprint_lengths_and_unknown_methods_are_discarded() {
    let server = Server::fixed().await;
    let client = server.client().await;
    let mut invalid = request(METHOD_BINDING, vec![], None);
    FINGERPRINT.add_to(&mut invalid).unwrap();
    let last = invalid.raw.len() - 1;
    invalid.raw[last] ^= 1;
    let mut trailing = request(METHOD_BINDING, vec![], None);
    trailing.raw.push(0);
    let unknown = request(METHOD_CONNECT, vec![], None);
    let mut invalid_length = request(METHOD_BINDING, vec![], None);
    invalid_length.raw[3] = 1;
    let mut buffer = [0; 2048];
    for message in [invalid, trailing, unknown, invalid_length] {
        client.send(&message.raw).await.unwrap();
        assert!(timeout(Duration::from_millis(40), client.recv(&mut buffer))
            .await
            .is_err());
    }
    assert_eq!(
        exchange(&client, &request(METHOD_BINDING, vec![], None))
            .await
            .typ,
        BINDING_SUCCESS
    );
    server.close().await;
}

#[tokio::test]
async fn external_revocation_releases_only_the_named_users_allocations() {
    let server = Server::fixed().await;
    let alice = server.client().await;
    let bob = server.client().await;
    let nonce = challenge(&alice).await;
    for (client, user) in [(&alice, "alice"), (&bob, "bob")] {
        let response = exchange(
            client,
            &request(
                METHOD_ALLOCATE,
                transport(),
                Some((user, PASSWORD, REALM, &nonce)),
            ),
        )
        .await;
        assert_eq!(response.typ.class, CLASS_SUCCESS_RESPONSE);
    }
    server
        .inner
        .delete_allocations_by_username("alice".into())
        .await
        .unwrap();
    let response = exchange(
        &alice,
        &request(
            METHOD_REFRESH,
            vec![],
            Some(("alice", PASSWORD, REALM, &nonce)),
        ),
    )
    .await;
    assert_error(&response, CODE_ALLOC_MISMATCH);
    let response = exchange(
        &bob,
        &request(
            METHOD_REFRESH,
            vec![],
            Some(("bob", PASSWORD, REALM, &nonce)),
        ),
    )
    .await;
    assert_eq!(response.typ.class, CLASS_SUCCESS_RESPONSE);
    server.close().await;
}

#[tokio::test]
async fn configured_relay_range_capacity_and_port_reuse() {
    let probe = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let port = probe.local_addr().unwrap().port();
    let socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let address = socket.local_addr().unwrap();
    let auth = Arc::new(
        turn::auth::StaticAuthHandler::new(BTreeMap::from([("alice".into(), PASSWORD.into())]))
            .unwrap(),
    );
    let inner = create_turn_server_with_port_range(socket, "127.0.0.1", REALM, auth, (port, port))
        .await
        .unwrap();
    let server = Server { inner, address };
    let client = server.client().await;
    let second = server.client().await;
    let nonce = challenge(&client).await;
    let auth = Some(("alice", PASSWORD, REALM, nonce.as_str()));
    drop(probe);
    let allocated = exchange(&client, &request(METHOD_ALLOCATE, transport(), auth)).await;
    assert_eq!(allocated.typ.class, CLASS_SUCCESS_RESPONSE);
    let mut relay = RelayedAddress::default();
    relay.get_from(&allocated).unwrap();
    assert_eq!(relay.port, port);
    assert_error(
        &exchange(&second, &request(METHOD_ALLOCATE, transport(), auth)).await,
        CODE_INSUFFICIENT_CAPACITY,
    );
    let deleted = exchange(
        &client,
        &request(
            METHOD_REFRESH,
            vec![Box::new(Lifetime(Duration::ZERO))],
            auth,
        ),
    )
    .await;
    assert_eq!(deleted.typ.class, CLASS_SUCCESS_RESPONSE);
    let reused = exchange(&second, &request(METHOD_ALLOCATE, transport(), auth)).await;
    assert_eq!(reused.typ.class, CLASS_SUCCESS_RESPONSE);
    relay.get_from(&reused).unwrap();
    assert_eq!(relay.port, port);
    server.close().await;
}

#[tokio::test]
async fn sha256_integrity_requires_a_full_turn_digest() {
    use sha2::Digest;
    let server = Server::fixed().await;
    let client = server.client().await;
    let nonce = challenge(&client).await;
    let key = sha2::Sha256::digest(format!("alice:{REALM}:{PASSWORD}").as_bytes());
    for length in [0, 4, 12, 16, 20, 24, 28, 36, 32] {
        let mut allocate = request(METHOD_ALLOCATE, transport(), None);
        allocate.add(ATTR_USERNAME, b"alice");
        allocate.add(ATTR_REALM, REALM.as_bytes());
        allocate.add(ATTR_NONCE, nonce.as_bytes());
        allocate.add(ATTR_PASSWORD_ALGORITHMS, &[0, 2, 0, 0, 0, 1, 0, 0]);
        allocate.add(ATTR_PASSWORD_ALGORITHM, &[0, 2, 0, 0]);
        let mut data = allocate.raw.clone();
        data[2..4].copy_from_slice(&((allocate.length + 4 + length) as u16).to_be_bytes());
        let mut mac = Hmac::<sha2::Sha256>::new_from_slice(&key).unwrap();
        mac.update(&data);
        let mut value = mac.finalize().into_bytes().to_vec();
        value.resize(length as usize, 0);
        allocate.add(ATTR_MESSAGE_INTEGRITY_SHA256, &value);
        let response = exchange(&client, &allocate).await;
        if length == 32 {
            assert_eq!(response.typ.class, CLASS_SUCCESS_RESPONSE);
            verify_sha256(&response, &key);
        } else {
            assert_error(&response, CODE_UNAUTHORIZED);
        }
    }
    server.close().await;
}

#[tokio::test]
async fn nonce_password_algorithm_feature_follows_rfc8489_wire_encoding() {
    let server = Server::fixed().await;
    let client = server.client().await;
    let response = exchange(&client, &request(METHOD_ALLOCATE, transport(), None)).await;
    let nonce = String::from_utf8(response.get(ATTR_NONCE).unwrap()).unwrap();
    let features = BASE64_STANDARD.decode(&nonce[9..13]).unwrap();
    server.close().await;
    // Section 9.2 explicitly puts feature 0 at the first byte's high bit.
    // Erratum 6290 changes section 18.1's registry numbering, not this wire
    // instruction. Preserve the wire format used by rustun pending clarification.
    assert_eq!(features, [0x80, 0, 0]);
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
        auth_handler: Arc::new(
            turn::auth::StaticAuthHandler::new(BTreeMap::from([("alice".into(), PASSWORD.into())]))
                .unwrap(),
        ),
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
            tokio::time::pause();
            tokio::time::advance(Duration::from_secs(29)).await;
            assert!(UdpSocket::bind(odd_address).await.is_err());
            tokio::time::advance(Duration::from_secs(2)).await;
            tokio::task::yield_now().await;
            tokio::time::resume();
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
async fn permission_validation_is_atomic_and_even_port_r0_has_no_token() {
    let server = Server::fixed().await;
    let client = server.client().await;
    let nonce = challenge(&client).await;
    let auth = Some(("alice", PASSWORD, REALM, nonce.as_str()));
    let mut attrs = transport();
    attrs.push(Box::new(Raw(ATTR_EVEN_PORT, vec![0])));
    let allocated = exchange(&client, &request(METHOD_ALLOCATE, attrs, auth)).await;
    assert_eq!(allocated.typ.class, CLASS_SUCCESS_RESPONSE);
    assert!(!allocated.contains(ATTR_RESERVATION_TOKEN));
    let mut relay = RelayedAddress::default();
    relay.get_from(&allocated).unwrap();
    assert_eq!(relay.port % 2, 0);
    let peer = UdpSocket::bind("127.0.0.2:0").await.unwrap();
    let peer_addr = peer.local_addr().unwrap();
    let rejected = exchange(
        &client,
        &request(
            METHOD_CREATE_PERMISSION,
            vec![
                Box::new(PeerAddress {
                    ip: peer_addr.ip(),
                    port: peer_addr.port(),
                }),
                Box::new(PeerAddress {
                    ip: "::1".parse().unwrap(),
                    port: 10000,
                }),
            ],
            auth,
        ),
    )
    .await;
    assert_error(&rejected, CODE_PEER_ADDR_FAMILY_MISMATCH);
    peer.send_to(b"must-not-relay", SocketAddr::new(relay.ip, relay.port))
        .await
        .unwrap();
    let mut buffer = [0; 2048];
    assert!(timeout(Duration::from_millis(50), client.recv(&mut buffer))
        .await
        .is_err());
    let accepted = exchange(
        &client,
        &request(
            METHOD_CREATE_PERMISSION,
            vec![Box::new(PeerAddress {
                ip: peer_addr.ip(),
                port: peer_addr.port(),
            })],
            auth,
        ),
    )
    .await;
    assert_eq!(accepted.typ.class, CLASS_SUCCESS_RESPONSE);
    peer.send_to(b"now-authorized", SocketAddr::new(relay.ip, relay.port))
        .await
        .unwrap();
    let len = timeout(WAIT, client.recv(&mut buffer))
        .await
        .unwrap()
        .unwrap();
    let mut indication = Message::new();
    indication.write(&buffer[..len]).unwrap();
    assert_eq!(indication.get(ATTR_DATA).unwrap(), b"now-authorized");
    server.close().await;
}

#[tokio::test]
async fn standalone_binding_uses_the_turn_datagram_validation() {
    use turn::server::request::handle_binding_datagram;
    let listener = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let client = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let src = client.local_addr().unwrap();
    let mut buffer = [0; 2048];
    let binding = request(METHOD_BINDING, vec![], None);
    handle_binding_datagram(listener.clone(), &binding.raw, src)
        .await
        .unwrap();
    let len = timeout(WAIT, client.recv(&mut buffer))
        .await
        .unwrap()
        .unwrap();
    let mut response = Message::new();
    response.write(&buffer[..len]).unwrap();
    assert_eq!(response.typ, BINDING_SUCCESS);
    assert_eq!(response.transaction_id, binding.transaction_id);
    FINGERPRINT.check(&response).unwrap();
    let mut mapped = XorMappedAddress::default();
    mapped.get_from(&response).unwrap();
    assert_eq!(SocketAddr::new(mapped.ip, mapped.port), src);

    let unknown = request(
        METHOD_BINDING,
        vec![Box::new(Raw(AttrType(0x1234), vec![]))],
        None,
    );
    handle_binding_datagram(listener.clone(), &unknown.raw, src)
        .await
        .unwrap();
    let len = timeout(WAIT, client.recv(&mut buffer))
        .await
        .unwrap()
        .unwrap();
    response.write(&buffer[..len]).unwrap();
    assert_error(&response, CODE_UNKNOWN_ATTRIBUTE);
    let mut attrs = UnknownAttributes(vec![]);
    attrs.get_from(&response).unwrap();
    assert_eq!(attrs.0, vec![AttrType(0x1234)]);

    let mut invalid_fingerprint = binding.clone();
    FINGERPRINT.add_to(&mut invalid_fingerprint).unwrap();
    *invalid_fingerprint.raw.last_mut().unwrap() ^= 1;
    let mut trailing = binding.raw.clone();
    trailing.extend_from_slice(&[0; 4]);
    let mut indication = binding.clone();
    indication.set_type(MessageType::new(METHOD_BINDING, CLASS_INDICATION));
    for raw in [
        invalid_fingerprint.raw,
        trailing,
        indication.raw,
        vec![0, 1],
        request(METHOD_ALLOCATE, transport(), None).raw,
    ] {
        handle_binding_datagram(listener.clone(), &raw, src)
            .await
            .unwrap();
    }
    assert!(
        timeout(Duration::from_millis(100), client.recv(&mut buffer))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn custom_dual_family_generator_reports_partial_capacity() {
    use turn::relay::{relay_static::RelayAddressGeneratorStatic, RelayAddressGenerator};
    use turn::server::config::{ConnConfig, ServerConfig};
    use util::{vnet::net::Net, Conn};
    struct DualFamily;
    #[async_trait::async_trait]
    impl RelayAddressGenerator for DualFamily {
        fn validate(&self) -> Result<(), turn::Error> {
            Ok(())
        }
        async fn allocate_conn(
            &self,
            ipv4: bool,
            port: u16,
        ) -> Result<(Arc<dyn Conn + Send + Sync>, SocketAddr), turn::Error> {
            let address = if ipv4 {
                format!("127.0.0.1:{port}")
            } else {
                format!("[::1]:{port}")
            };
            let socket = Arc::new(UdpSocket::bind(address).await?);
            let address = socket.local_addr()?;
            Ok((socket, address))
        }
    }
    let socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let address = socket.local_addr().unwrap();
    let inner = turn::server::Server::new(ServerConfig {
        conn_configs: vec![ConnConfig {
            relay_addr_generator: Box::new(DualFamily),
            conn: socket,
        }],
        realm: REALM.into(),
        auth_handler: Arc::new(
            turn::auth::StaticAuthHandler::new(BTreeMap::from([("alice".into(), PASSWORD.into())]))
                .unwrap(),
        ),
        channel_bind_timeout: Duration::from_secs(600),
        alloc_close_notify: None,
    })
    .await
    .unwrap();
    let server = Server { inner, address };
    let client = server.client().await;
    let nonce = challenge(&client).await;
    let mut attrs = transport();
    attrs.push(Box::new(Raw(AttrType(0x8000), vec![2, 0, 0, 0])));
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
    assert_eq!(
        &allocated.get(AttrType(0x8001)).unwrap()[..4],
        &[2, 0, 5, 8]
    );
    assert_integrity(&allocated, "alice", PASSWORD);
    server.close().await;

    let generator = RelayAddressGeneratorStatic {
        relay_address: "127.0.0.1".parse().unwrap(),
        address: "127.0.0.1".into(),
        net: Arc::new(Net::new(None)),
    };
    assert!(matches!(
        generator.allocate_conn(false, 0).await,
        Err(turn::Error::ErrInvalidRequestedFamilyValue)
    ));
}
