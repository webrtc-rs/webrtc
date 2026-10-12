#[cfg(test)]
mod request_test;

use std::collections::HashMap;
use std::marker::{Send, Sync};
use std::net::SocketAddr;
#[cfg(feature = "metrics")]
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::SystemTime;

use super::integrity::Integrity;
use md5::{Digest, Md5};
use stun::agent::*;
use stun::attributes::*;
use stun::error_code::*;
use stun::fingerprint::*;
#[cfg(test)]
use stun::integrity::*;
use stun::message::*;
use stun::textattrs::*;
use stun::uattrs::*;
use stun::xoraddr::*;
use tokio::sync::Mutex;
use tokio::time::{Duration, Instant};
use util::Conn;

use crate::allocation::allocation_manager::*;
use crate::allocation::channel_bind::ChannelBind;
use crate::allocation::five_tuple::*;
use crate::allocation::permission::Permission;
use crate::auth::*;
use crate::error::*;
use crate::proto::chandata::ChannelData;
use crate::proto::channum::ChannelNumber;
use crate::proto::data::Data;
use crate::proto::evenport::EvenPort;
use crate::proto::lifetime::*;
use crate::proto::peeraddr::PeerAddress;
use crate::proto::relayaddr::RelayedAddress;
use crate::proto::reqfamily::{RequestedAddressFamily, REQUESTED_FAMILY_IPV4};
use crate::proto::reqtrans::RequestedTransport;
use crate::proto::rsrvtoken::ReservationToken;
use crate::proto::*;

pub(crate) const MAXIMUM_ALLOCATION_LIFETIME: Duration = Duration::from_secs(3600); // https://tools.ietf.org/html/rfc5766#section-6.2 defines 3600 seconds recommendation
pub(crate) const NONCE_LIFETIME: Duration = Duration::from_secs(3600); // https://tools.ietf.org/html/rfc5766#section-4

/// Request contains all the state needed to process a single incoming datagram
pub struct Request {
    // Current Request State
    pub conn: Arc<dyn Conn + Send + Sync>,
    pub src_addr: SocketAddr,
    pub buff: Vec<u8>,

    // Server State
    pub allocation_manager: Arc<Manager>,
    pub nonces: Arc<Mutex<HashMap<String, Instant>>>,

    // User Configuration
    pub auth_handler: Arc<dyn AuthHandler + Send + Sync>,
    pub realm: String,
    pub channel_bind_timeout: Duration,
}

impl Request {
    pub fn new(
        conn: Arc<dyn Conn + Send + Sync>,
        src_addr: SocketAddr,
        allocation_manager: Arc<Manager>,
        auth_handler: Arc<dyn AuthHandler + Send + Sync>,
    ) -> Self {
        Request {
            conn,
            src_addr,
            buff: vec![],
            allocation_manager,
            nonces: Arc::new(Mutex::new(HashMap::new())),
            auth_handler,
            realm: String::new(),
            channel_bind_timeout: Duration::from_secs(0),
        }
    }

    /// Processes the give [`Request`]
    pub async fn handle_request(&mut self) -> Result<()> {
        /*log::debug!(
            "received {} bytes of udp from {} on {}",
            self.buff.len(),
            self.src_addr,
            self.conn.local_addr().await?
        );*/

        if ChannelData::is_channel_data(&self.buff) {
            self.handle_data_packet().await
        } else if is_stun_datagram(&self.buff) {
            self.handle_turn_packet().await
        } else {
            Ok(())
        }
    }

    async fn handle_data_packet(&mut self) -> Result<()> {
        log::debug!("received DataPacket from {}", self.src_addr);
        let mut c = ChannelData {
            raw: self.buff.clone(),
            ..Default::default()
        };
        c.decode()?;
        self.handle_channel_data(&c).await
    }

    async fn handle_turn_packet(&mut self) -> Result<()> {
        log::debug!("handle_turn_packet");
        let Some(m) = decode_stun_datagram(&self.buff)? else {
            return Ok(());
        };
        self.process_message_handler(&m).await
    }

    async fn process_message_handler(&mut self, m: &Message) -> Result<()> {
        if m.typ.class == CLASS_INDICATION {
            match m.typ.method {
                METHOD_SEND => {
                    if self.reject_unknown_attributes(m, None).await? {
                        return Ok(());
                    }
                    self.handle_send_indication(m).await
                }
                _ => Err(Error::ErrUnexpectedClass),
            }
        } else if m.typ.class == CLASS_REQUEST {
            match m.typ.method {
                METHOD_ALLOCATE => self.handle_allocate_request(m).await,
                METHOD_REFRESH => self.handle_refresh_request(m).await,
                METHOD_CREATE_PERMISSION => self.handle_create_permission_request(m).await,
                METHOD_CHANNEL_BIND => self.handle_channel_bind_request(m).await,
                METHOD_BINDING => self.handle_binding_request(m).await,
                _ => Ok(()),
            }
        } else {
            Err(Error::ErrUnexpectedClass)
        }
    }

    pub(crate) async fn authenticate_request(
        &mut self,
        m: &Message,
        calling_method: Method,
    ) -> Result<Option<(Username, Integrity)>> {
        if !m.contains(ATTR_MESSAGE_INTEGRITY) && !m.contains(ATTR_MESSAGE_INTEGRITY_SHA256) {
            self.respond_with_nonce(m, calling_method, CODE_UNAUTHORIZED)
                .await?;
            return Ok(None);
        }
        let mut nonce = Nonce::new(ATTR_NONCE, String::new());
        let mut username = Username::new(ATTR_USERNAME, String::new());
        let mut realm = Realm::new(ATTR_REALM, String::new());
        if nonce.get_from(m).is_err() || username.get_from(m).is_err() || realm.get_from(m).is_err()
        {
            self.respond_error(m, CODE_BAD_REQUEST, None).await?;
            return Ok(None);
        }
        let algorithms = m.get(ATTR_PASSWORD_ALGORITHMS).ok();
        let algorithm = m.get(ATTR_PASSWORD_ALGORITHM).ok();
        let negotiated = algorithms.is_some() || algorithm.is_some();
        let supported = self.auth_handler.password_algorithms();
        let advertised = password_algorithms_value(&supported);
        let password_algorithm = algorithm
            .as_ref()
            .and_then(|value| {
                (value.len() == 4 && value[2..] == [0, 0])
                    .then(|| u16::from_be_bytes([value[0], value[1]]))
            })
            .unwrap_or(1);
        if negotiated
            && (algorithms.as_deref() != Some(advertised.as_slice())
                || algorithm.is_none()
                || !supported.contains(&password_algorithm)
                || algorithm
                    .as_ref()
                    .is_some_and(|value| value.len() != 4 || value[2..] != [0, 0]))
        {
            self.respond_error(m, CODE_BAD_REQUEST, None).await?;
            return Ok(None);
        }
        if realm.text != self.realm {
            self.respond_with_nonce(m, calling_method, CODE_UNAUTHORIZED)
                .await?;
            return Ok(None);
        }
        let five_tuple = FiveTuple {
            src_addr: self.src_addr,
            dst_addr: self.conn.local_addr()?,
            protocol: PROTO_UDP,
        };
        let allocation = self.allocation_manager.get_allocation(&five_tuple).await;
        // Keep the original credential key for this allocation. REST credentials
        // expiring prevents new allocations, not authenticated refreshes of old ones.
        let cached_key = allocation.as_ref().and_then(|a| {
            if a.username() == username.text {
                a.auth_keys.lock().get(&password_algorithm).cloned()
            } else {
                None
            }
        });
        let key = match cached_key {
            Some(key) => key,
            None => match self.auth_handler.auth_handle_algorithm(
                &username.text,
                &self.realm,
                self.src_addr,
                password_algorithm,
            ) {
                Ok(key) => key,
                Err(_) => {
                    self.respond_with_nonce(m, calling_method, CODE_UNAUTHORIZED)
                        .await?;
                    return Ok(None);
                }
            },
        };
        let mut integrity = Integrity {
            key,
            password_algorithm,
            sha256: m.contains(ATTR_MESSAGE_INTEGRITY_SHA256),
        };
        if integrity.check(m).is_err() {
            self.respond_with_nonce(m, calling_method, CODE_UNAUTHORIZED)
                .await?;
            return Ok(None);
        }
        integrity.sha256 = negotiated;
        // Verify integrity before reporting a stale nonce (RFC 8489, 9.2.4).
        let stale = {
            let mut nonces = self.nonces.lock().await;
            nonces.retain(|_, created| created.elapsed() < NONCE_LIFETIME);
            !nonces.contains_key(&nonce.text)
        };
        if stale {
            self.respond_with_nonce(m, calling_method, CODE_STALE_NONCE)
                .await?;
            return Ok(None);
        }
        if calling_method == METHOD_ALLOCATE && forbidden_tunnel(self.src_addr.ip()) {
            self.respond_error(m, CODE_FORBIDDEN, Some(integrity))
                .await?;
            return Ok(None);
        }
        if calling_method != METHOD_ALLOCATE {
            match allocation {
                None => {
                    self.respond_error(m, CODE_ALLOC_MISMATCH, Some(integrity))
                        .await?;
                    return Ok(None);
                }
                Some(a) if a.username() != username.text => {
                    self.respond_error(m, CODE_WRONG_CREDENTIALS, Some(integrity))
                        .await?;
                    return Ok(None);
                }
                _ => {}
            }
        }
        if self.reject_unknown_attributes(m, Some(&integrity)).await? {
            return Ok(None);
        }
        Ok(Some((username, integrity)))
    }

    async fn reject_unknown_attributes(
        &self,
        m: &Message,
        integrity: Option<&Integrity>,
    ) -> Result<bool> {
        let known = [
            ATTR_USERNAME,
            ATTR_REALM,
            ATTR_NONCE,
            ATTR_MESSAGE_INTEGRITY,
            ATTR_MESSAGE_INTEGRITY_SHA256,
            ATTR_PASSWORD_ALGORITHM,
            ATTR_CHANNEL_NUMBER,
            ATTR_LIFETIME,
            ATTR_XOR_PEER_ADDRESS,
            ATTR_DATA,
            ATTR_EVEN_PORT,
            ATTR_REQUESTED_TRANSPORT,
            ATTR_RESERVATION_TOKEN,
            ATTR_REQUESTED_ADDRESS_FAMILY,
        ];
        let mut unknown: Vec<AttrType> = m
            .attributes
            .0
            .iter()
            .filter(|a| a.typ.required() && !known.contains(&a.typ))
            .map(|a| a.typ)
            .collect();
        unknown.sort_by_key(|a| a.0);
        unknown.dedup();
        if unknown.is_empty() {
            return Ok(false);
        }
        if m.typ.class == CLASS_REQUEST {
            let response = {
                let mut attrs: Vec<Box<dyn Setter>> = vec![
                    Box::new(ErrorCodeAttribute {
                        code: CODE_UNKNOWN_ATTRIBUTE,
                        reason: vec![],
                    }),
                    Box::new(UnknownAttributes(unknown)),
                ];
                if let Some(integrity) = integrity {
                    attrs.push(Box::new(integrity.clone()));
                }
                build_msg(
                    m.transaction_id,
                    MessageType::new(m.typ.method, CLASS_ERROR_RESPONSE),
                    attrs,
                )?
            };
            build_and_send(&self.conn, self.src_addr, response).await?;
        }
        Ok(true)
    }

    async fn respond_error(
        &self,
        m: &Message,
        code: ErrorCode,
        integrity: Option<Integrity>,
    ) -> Result<()> {
        let response = {
            let mut attrs: Vec<Box<dyn Setter>> = vec![Box::new(ErrorCodeAttribute {
                code,
                reason: vec![],
            })];
            if let Some(integrity) = integrity {
                attrs.push(Box::new(integrity));
            }
            build_msg(
                m.transaction_id,
                MessageType::new(m.typ.method, CLASS_ERROR_RESPONSE),
                attrs,
            )?
        };
        build_and_send(&self.conn, self.src_addr, response).await
    }

    async fn respond_with_nonce(
        &mut self,
        m: &Message,
        calling_method: Method,
        response_code: ErrorCode,
    ) -> Result<()> {
        let nonce = build_nonce()?;

        {
            // Nonce has already been taken
            let mut nonces = self.nonces.lock().await;
            nonces.retain(|_, created| created.elapsed() < NONCE_LIFETIME);
            if nonces.contains_key(&nonce) {
                return Err(Error::ErrDuplicatedNonce);
            }
            nonces.insert(nonce.clone(), Instant::now());
        }

        let msg = build_msg(
            m.transaction_id,
            MessageType::new(calling_method, CLASS_ERROR_RESPONSE),
            vec![
                Box::new(ErrorCodeAttribute {
                    code: response_code,
                    reason: vec![],
                }),
                Box::new(Nonce::new(ATTR_NONCE, nonce)),
                Box::new(Realm::new(ATTR_REALM, self.realm.clone())),
                Box::new(PasswordAlgorithms(self.auth_handler.password_algorithms())),
            ],
        )?;

        build_and_send(&self.conn, self.src_addr, msg).await
    }

    pub(crate) async fn handle_binding_request(&mut self, m: &Message) -> Result<()> {
        log::debug!("received BindingRequest from {}", self.src_addr);
        if self.reject_unknown_attributes(m, None).await? {
            return Ok(());
        }

        let (ip, port) = (self.src_addr.ip(), self.src_addr.port());

        let msg = build_msg(
            m.transaction_id,
            BINDING_SUCCESS,
            vec![
                Box::new(XorMappedAddress { ip, port }),
                Box::new(FINGERPRINT),
            ],
        )?;

        build_and_send(&self.conn, self.src_addr, msg).await
    }

    /// RFC 8656 section 7.2, with one configured relay address family.
    pub(crate) async fn handle_allocate_request(&mut self, m: &Message) -> Result<()> {
        let Some((username, integrity)) = self.authenticate_request(m, METHOD_ALLOCATE).await?
        else {
            return Ok(());
        };
        let five_tuple = FiveTuple {
            src_addr: self.src_addr,
            dst_addr: self.conn.local_addr()?,
            protocol: PROTO_UDP,
        };
        if let Some(allocation) = self.allocation_manager.get_allocation(&five_tuple).await {
            let cached = allocation.allocate_response.lock().await.clone();
            if let Some(response) = cached {
                if response.transaction_id == m.transaction_id
                    && allocation.username() == username.text
                {
                    self.conn.send_to(&response.raw, self.src_addr).await?;
                    return Ok(());
                }
            }
            return self
                .respond_error(m, CODE_ALLOC_MISMATCH, Some(integrity))
                .await;
        }
        let mut transport = RequestedTransport::default();
        if transport.get_from(m).is_err() {
            return self
                .respond_error(m, CODE_BAD_REQUEST, Some(integrity))
                .await;
        }
        if transport.protocol != PROTO_UDP {
            return self
                .respond_error(m, CODE_UNSUPPORTED_TRANS_PROTO, Some(integrity))
                .await;
        }
        let has_token = m.contains(ATTR_RESERVATION_TOKEN);
        let has_family = m.contains(ATTR_REQUESTED_ADDRESS_FAMILY);
        let additional = m.get(AttrType(0x8000)).ok();
        let has_even = m.contains(ATTR_EVEN_PORT);
        let mut even = EvenPort::default();
        let mut token = ReservationToken::default();
        let mut family = REQUESTED_FAMILY_IPV4;
        if (has_token && (has_even || has_family || additional.is_some()))
            || (has_family && additional.is_some())
            || (has_even && even.get_from(m).is_err())
            || (has_token && token.get_from(m).is_err())
            || (m.contains(ATTR_LIFETIME) && m.get(ATTR_LIFETIME)?.len() != 4)
            || additional
                .as_ref()
                .is_some_and(|value| value.len() != 4 || value[0] != 2)
            || (additional.is_some() && has_even && even.reserve_port)
        {
            return self
                .respond_error(m, CODE_BAD_REQUEST, Some(integrity))
                .await;
        }
        if has_family {
            if m.get(ATTR_REQUESTED_ADDRESS_FAMILY)?.len() != 4 {
                return self
                    .respond_error(m, CODE_BAD_REQUEST, Some(integrity))
                    .await;
            }
            if family.get_from(m).is_err() {
                return self
                    .respond_error(m, CODE_ADDR_FAMILY_NOT_SUPPORTED, Some(integrity))
                    .await;
            }
        }
        let mut use_ipv4 = family == REQUESTED_FAMILY_IPV4;
        if additional.is_some() && !self.allocation_manager.supports_family(true) {
            use_ipv4 = false;
        }
        if !has_token && !self.allocation_manager.supports_family(use_ipv4) {
            return self
                .respond_error(m, CODE_ADDR_FAMILY_NOT_SUPPORTED, Some(integrity))
                .await;
        }
        let (relay, reserved) = if has_token {
            let Some(relay) = self.allocation_manager.take_reserved_relay(&token.0).await else {
                return self
                    .respond_error(m, CODE_INSUFFICIENT_CAPACITY, Some(integrity))
                    .await;
            };
            (relay, None)
        } else {
            let result = if has_even {
                self.allocation_manager
                    .allocate_even_relay(use_ipv4, even.reserve_port)
                    .await
            } else {
                self.allocation_manager
                    .allocate_relay(use_ipv4)
                    .await
                    .map(|relay| (relay, None))
            };
            match result {
                Ok(result) => result,
                Err(_) => {
                    return self
                        .respond_error(m, CODE_INSUFFICIENT_CAPACITY, Some(integrity))
                        .await;
                }
            }
        };
        let lifetime = allocation_lifetime(m).max(DEFAULT_LIFETIME);
        let allocation = self
            .allocation_manager
            .create_allocation_with_relay(
                five_tuple,
                Arc::clone(&self.conn),
                lifetime,
                username,
                relay,
            )
            .await?;
        allocation
            .auth_keys
            .lock()
            .insert(integrity.password_algorithm, integrity.key.clone());
        // Cache all configured derivations at admission so credential expiry does
        // not interrupt an established allocation's authentication negotiation.
        for algorithm in self.auth_handler.password_algorithms() {
            if algorithm == integrity.password_algorithm {
                continue;
            }
            if let Ok(key) = self.auth_handler.auth_handle_algorithm(
                allocation.username(),
                &self.realm,
                self.src_addr,
                algorithm,
            ) {
                allocation.auth_keys.lock().insert(algorithm, key);
            }
        }
        let msg = {
            let mut attrs: Vec<Box<dyn Setter>> = vec![
                Box::new(RelayedAddress {
                    ip: allocation.relay_addr.ip(),
                    port: allocation.relay_addr.port(),
                }),
                Box::new(Lifetime(lifetime)),
                Box::new(XorMappedAddress {
                    ip: self.src_addr.ip(),
                    port: self.src_addr.port(),
                }),
            ];
            if let Some(token) = reserved {
                attrs.push(Box::new(ReservationToken(token)));
            }
            if additional.is_some() {
                // This allocation stores one relay socket. Report capacity, rather
                // than an unsupported family, when a custom generator supports both.
                let code = if self.allocation_manager.supports_family(!use_ipv4) {
                    CODE_INSUFFICIENT_CAPACITY
                } else {
                    CODE_ADDR_FAMILY_NOT_SUPPORTED
                };
                attrs.push(Box::new(AddressError {
                    family: if use_ipv4 { 2 } else { 1 },
                    code,
                }));
            }
            attrs.push(Box::new(integrity));
            build_msg(
                m.transaction_id,
                MessageType::new(METHOD_ALLOCATE, CLASS_SUCCESS_RESPONSE),
                attrs,
            )?
        };
        *allocation.allocate_response.lock().await = Some(msg.clone());
        build_and_send(&self.conn, self.src_addr, msg).await
    }

    pub(crate) async fn handle_refresh_request(&mut self, m: &Message) -> Result<()> {
        let Some((_, integrity)) = self.authenticate_request(m, METHOD_REFRESH).await? else {
            return Ok(());
        };
        let five_tuple = FiveTuple {
            src_addr: self.src_addr,
            dst_addr: self.conn.local_addr()?,
            protocol: PROTO_UDP,
        };
        let allocation = self
            .allocation_manager
            .get_allocation(&five_tuple)
            .await
            .ok_or(Error::ErrNoAllocationFound)?;
        if m.contains(ATTR_LIFETIME) && m.get(ATTR_LIFETIME)?.len() != 4 {
            return self
                .respond_error(m, CODE_BAD_REQUEST, Some(integrity))
                .await;
        }
        if m.contains(ATTR_REQUESTED_ADDRESS_FAMILY) {
            let mut family = RequestedAddressFamily::default();
            if family.get_from(m).is_err() {
                return self
                    .respond_error(m, CODE_BAD_REQUEST, Some(integrity))
                    .await;
            }
            if (family == REQUESTED_FAMILY_IPV4) != allocation.relay_addr.is_ipv4() {
                return self
                    .respond_error(m, CODE_PEER_ADDR_FAMILY_MISMATCH, Some(integrity))
                    .await;
            }
        }
        let lifetime = allocation_lifetime(m);
        if lifetime.is_zero() {
            self.allocation_manager.delete_allocation(&five_tuple).await;
        } else {
            allocation.refresh(lifetime).await;
        }
        let response = build_msg(
            m.transaction_id,
            MessageType::new(METHOD_REFRESH, CLASS_SUCCESS_RESPONSE),
            vec![Box::new(Lifetime(lifetime)), Box::new(integrity)],
        )?;
        build_and_send(&self.conn, self.src_addr, response).await
    }

    pub(crate) async fn handle_create_permission_request(&mut self, m: &Message) -> Result<()> {
        log::debug!("received CreatePermission from {}", self.src_addr);

        let (_, message_integrity) = if let Some(mi) = self
            .authenticate_request(m, METHOD_CREATE_PERMISSION)
            .await?
        {
            mi
        } else {
            log::debug!("no MessageIntegrity");
            return Ok(());
        };
        let a = self
            .allocation_manager
            .get_allocation(&FiveTuple {
                src_addr: self.src_addr,
                dst_addr: self.conn.local_addr()?,
                protocol: PROTO_UDP,
            })
            .await
            .ok_or(Error::ErrNoAllocationFound)?;
        let mut peers = Vec::new();
        for attr in &m.attributes.0 {
            if attr.typ != ATTR_XOR_PEER_ADDRESS {
                continue;
            }
            let mut peer_message = Message::new();
            peer_message.transaction_id = m.transaction_id;
            peer_message.add(ATTR_XOR_PEER_ADDRESS, &attr.value);
            let mut peer = PeerAddress::default();
            if peer.get_from(&peer_message).is_err() {
                self.respond_error(m, CODE_BAD_REQUEST, Some(message_integrity))
                    .await?;
                return Ok(());
            }
            if peer.ip.is_ipv4() != a.relay_addr.is_ipv4() {
                self.respond_error(m, CODE_PEER_ADDR_FAMILY_MISMATCH, Some(message_integrity))
                    .await?;
                return Ok(());
            }
            if forbidden_tunnel(peer.ip) {
                return self
                    .respond_error(m, CODE_FORBIDDEN, Some(message_integrity))
                    .await;
            }
            peers.push(SocketAddr::new(peer.ip, peer.port));
        }
        if peers.is_empty() {
            self.respond_error(m, CODE_BAD_REQUEST, Some(message_integrity))
                .await?;
            return Ok(());
        }
        for peer in peers {
            a.add_permission(Permission::new(peer)).await;
        }
        let msg = build_msg(
            m.transaction_id,
            MessageType::new(METHOD_CREATE_PERMISSION, CLASS_SUCCESS_RESPONSE),
            vec![Box::new(message_integrity)],
        )?;
        build_and_send(&self.conn, self.src_addr, msg).await
    }

    pub(crate) async fn handle_send_indication(&mut self, m: &Message) -> Result<()> {
        log::debug!("received SendIndication from {}", self.src_addr);

        let a = self
            .allocation_manager
            .get_allocation(&FiveTuple {
                src_addr: self.src_addr,
                dst_addr: self.conn.local_addr()?,
                protocol: PROTO_UDP,
            })
            .await
            .ok_or(Error::ErrNoAllocationFound)?;

        if m.contains(ATTR_DONT_FRAGMENT) {
            return Ok(());
        }
        let mut data_attr = Data::default();
        data_attr.get_from(m)?;

        let mut peer_address = PeerAddress::default();
        peer_address.get_from(m)?;

        if peer_address.ip.is_ipv4() != a.relay_addr.is_ipv4() {
            return Ok(());
        }
        if forbidden_tunnel(peer_address.ip) {
            return Ok(());
        }
        let msg_dst = SocketAddr::new(peer_address.ip, peer_address.port);

        let has_perm = a.has_permission(&msg_dst).await;
        if !has_perm {
            return Err(Error::ErrNoPermission);
        }

        let l = a.relay_socket.send_to(&data_attr.0, msg_dst).await?;
        if l != data_attr.0.len() {
            Err(Error::ErrShortWrite)
        } else {
            #[cfg(feature = "metrics")]
            a.relayed_bytes
                .fetch_add(data_attr.0.len(), Ordering::AcqRel);

            Ok(())
        }
    }

    pub(crate) async fn handle_channel_bind_request(&mut self, m: &Message) -> Result<()> {
        let Some((_, integrity)) = self.authenticate_request(m, METHOD_CHANNEL_BIND).await? else {
            return Ok(());
        };
        let allocation = self
            .allocation_manager
            .get_allocation(&FiveTuple {
                src_addr: self.src_addr,
                dst_addr: self.conn.local_addr()?,
                protocol: PROTO_UDP,
            })
            .await
            .ok_or(Error::ErrNoAllocationFound)?;
        let mut channel = ChannelNumber::default();
        let mut peer = PeerAddress::default();
        if channel.get_from(m).is_err() || !channel.valid() || peer.get_from(m).is_err() {
            return self
                .respond_error(m, CODE_BAD_REQUEST, Some(integrity))
                .await;
        }
        if peer.ip.is_ipv4() != allocation.relay_addr.is_ipv4() {
            return self
                .respond_error(m, CODE_PEER_ADDR_FAMILY_MISMATCH, Some(integrity))
                .await;
        }
        if forbidden_tunnel(peer.ip) {
            return self.respond_error(m, CODE_FORBIDDEN, Some(integrity)).await;
        }
        if allocation
            .add_channel_bind(
                ChannelBind::new(channel, SocketAddr::new(peer.ip, peer.port)),
                self.channel_bind_timeout,
            )
            .await
            .is_err()
        {
            return self
                .respond_error(m, CODE_BAD_REQUEST, Some(integrity))
                .await;
        }
        let response = build_msg(
            m.transaction_id,
            MessageType::new(METHOD_CHANNEL_BIND, CLASS_SUCCESS_RESPONSE),
            vec![Box::new(integrity)],
        )?;
        build_and_send(&self.conn, self.src_addr, response).await
    }

    pub(crate) async fn handle_channel_data(&mut self, c: &ChannelData) -> Result<()> {
        log::debug!("received ChannelData from {}", self.src_addr);

        let a = self
            .allocation_manager
            .get_allocation(&FiveTuple {
                src_addr: self.src_addr,
                dst_addr: self.conn.local_addr()?,
                protocol: PROTO_UDP,
            })
            .await
            .ok_or(Error::ErrNoAllocationFound)?;

        let channel = a.get_channel_addr(&c.number).await;
        if let Some(peer) = channel {
            let l = a.relay_socket.send_to(&c.data, peer).await?;
            if l != c.data.len() {
                Err(Error::ErrShortWrite)
            } else {
                #[cfg(feature = "metrics")]
                a.relayed_bytes.fetch_add(c.data.len(), Ordering::AcqRel);

                Ok(())
            }
        } else {
            Err(Error::ErrNoSuchChannelBind)
        }
    }
}

/// Identify STUN framing; callers must still decode and validate the complete datagram.
pub fn is_stun_datagram(data: &[u8]) -> bool {
    data.first().is_some_and(|first| first & 0xc0 == 0)
}

fn decode_stun_datagram(data: &[u8]) -> Result<Option<Message>> {
    let mut m = Message {
        raw: data.to_vec(),
        ..Default::default()
    };
    m.decode()?;
    if m.raw.len() != 20 + m.length as usize || !m.length.is_multiple_of(4) {
        return Ok(None);
    }
    if m.contains(ATTR_FINGERPRINT)
        && (m.attributes.0.last().map(|a| a.typ) != Some(ATTR_FINGERPRINT)
            || FINGERPRINT.check(&m).is_err())
    {
        return Ok(None);
    }
    // Attributes after MESSAGE-INTEGRITY are not authenticated and must not
    // affect permissions, lifetime or routing (RFC 8489, 9.2.4).
    if let Some(index) = m
        .attributes
        .0
        .iter()
        .position(|a| a.typ == ATTR_MESSAGE_INTEGRITY || a.typ == ATTR_MESSAGE_INTEGRITY_SHA256)
    {
        // SHA-256 integrity is the only authenticated attribute allowed
        // after legacy integrity. Keep its wire bytes for verification while
        // hiding every intervening semantic attribute from the handlers.
        let sha256 = m
            .attributes
            .0
            .iter()
            .position(|a| a.typ == ATTR_MESSAGE_INTEGRITY_SHA256);
        let last = sha256.unwrap_or(index).max(index);
        let end = 20
            + m.attributes.0[..=last]
                .iter()
                .map(|a| 4 + (a.length as usize).div_ceil(4) * 4)
                .sum::<usize>();
        m.attributes.0 = m
            .attributes
            .0
            .iter()
            .enumerate()
            .filter(|(i, _)| *i <= index || Some(*i) == sha256)
            .map(|(_, attr)| attr.clone())
            .collect();
        m.raw.truncate(end);
        m.length = (end - 20) as u32;
        m.write_length();
    }
    Ok(Some(m))
}

/// Process an unauthenticated Binding datagram using the same RFC 8489 validation
/// as the TURN listener. Other methods and malformed datagrams are ignored.
pub async fn handle_binding_datagram(
    conn: Arc<dyn Conn + Send + Sync>,
    data: &[u8],
    src_addr: SocketAddr,
) -> Result<()> {
    if !is_stun_datagram(data) {
        return Ok(());
    }
    let Ok(Some(m)) = decode_stun_datagram(data) else {
        return Ok(());
    };
    if m.typ != BINDING_REQUEST {
        return Ok(());
    }
    let known = [
        ATTR_USERNAME,
        ATTR_REALM,
        ATTR_NONCE,
        ATTR_MESSAGE_INTEGRITY,
        ATTR_MESSAGE_INTEGRITY_SHA256,
        ATTR_PASSWORD_ALGORITHM,
    ];
    let mut unknown: Vec<_> = m
        .attributes
        .0
        .iter()
        .filter(|a| a.typ.required() && !known.contains(&a.typ))
        .map(|a| a.typ)
        .collect();
    unknown.sort_by_key(|a| a.0);
    unknown.dedup();
    let response = if unknown.is_empty() {
        build_msg(
            m.transaction_id,
            BINDING_SUCCESS,
            vec![
                Box::new(XorMappedAddress {
                    ip: src_addr.ip(),
                    port: src_addr.port(),
                }),
                Box::new(FINGERPRINT),
            ],
        )?
    } else {
        build_msg(
            m.transaction_id,
            MessageType::new(METHOD_BINDING, CLASS_ERROR_RESPONSE),
            vec![
                Box::new(ErrorCodeAttribute {
                    code: CODE_UNKNOWN_ATTRIBUTE,
                    reason: vec![],
                }),
                Box::new(UnknownAttributes(unknown)),
            ],
        )?
    };
    build_and_send(&conn, src_addr, response).await
}

// RFC 8656 section 21.4 forbids relaying to Teredo and 6to4 tunnel endpoints.
fn forbidden_tunnel(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V6(ip) => {
            let segments = ip.segments();
            segments[0] == 0x2002 || (segments[0] == 0x2001 && segments[1] == 0)
        }
        _ => false,
    }
}

struct AddressError {
    family: u8,
    code: ErrorCode,
}
impl Setter for AddressError {
    fn add_to(&self, message: &mut Message) -> std::result::Result<(), stun::Error> {
        let mut value = vec![
            self.family,
            0,
            (self.code.0 / 100) as u8,
            (self.code.0 % 100) as u8,
        ];
        value.extend_from_slice(if self.code == CODE_INSUFFICIENT_CAPACITY {
            b"Insufficient Capacity"
        } else {
            b"Address Family not Supported"
        });
        message.add(AttrType(0x8001), &value);
        Ok(())
    }
}

fn password_algorithms_value(algorithms: &[u16]) -> Vec<u8> {
    algorithms
        .iter()
        .flat_map(|algorithm| {
            let bytes = algorithm.to_be_bytes();
            [bytes[0], bytes[1], 0, 0]
        })
        .collect()
}

struct PasswordAlgorithms(Vec<u16>);
impl Setter for PasswordAlgorithms {
    fn add_to(&self, message: &mut Message) -> std::result::Result<(), stun::Error> {
        message.add(
            ATTR_PASSWORD_ALGORITHMS,
            &password_algorithms_value(&self.0),
        );
        Ok(())
    }
}

pub(crate) fn rand_seq(n: usize) -> String {
    use rand::distr::{Alphabetic, SampleString};

    Alphabetic.sample_string(&mut rand::rng(), n)
}

pub(crate) fn build_nonce() -> Result<String> {
    /* #nosec */
    let mut s = String::new();
    s.push_str(
        format!(
            "{}",
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)?
                .as_nanos()
        )
        .as_str(),
    );
    s.push_str(format!("{}", rand::random::<u64>()).as_str());

    let mut h = Md5::new();
    h.update(s.as_bytes());
    // RFC 8489 section 9.2 explicitly encodes feature 0 as 80 00 00.
    // Erratum 6290 changes section 18.1's registry numbering but leaves
    // section 9.2 unchanged; retain its wire format (also used by rustun).
    Ok(format!("obMatJos2gAAA{:x}", h.finalize()))
}

pub(crate) async fn build_and_send(
    conn: &Arc<dyn Conn + Send + Sync>,
    dst: SocketAddr,
    msg: Message,
) -> Result<()> {
    let _ = conn.send_to(&msg.raw, dst).await?;
    Ok(())
}

/// Send a STUN packet and return the original error to the caller
pub(crate) async fn build_and_send_err(
    conn: &Arc<dyn Conn + Send + Sync>,
    dst: SocketAddr,
    msg: Message,
    err: Error,
) -> Result<()> {
    build_and_send(conn, dst, msg).await?;

    Err(err)
}

pub(crate) fn build_msg(
    transaction_id: TransactionId,
    msg_type: MessageType,
    mut additional: Vec<Box<dyn Setter>>,
) -> Result<Message> {
    let mut attrs: Vec<Box<dyn Setter>> = vec![
        Box::new(Message {
            transaction_id,
            ..Default::default()
        }),
        Box::new(msg_type),
    ];

    attrs.append(&mut additional);

    let mut msg = Message::new();
    msg.build(&attrs)?;
    Ok(msg)
}

pub(crate) fn allocation_lifetime(m: &Message) -> Duration {
    let mut lifetime = Lifetime::default();
    if lifetime.get_from(m).is_err() {
        return DEFAULT_LIFETIME;
    }
    if lifetime.0.is_zero() {
        return Duration::ZERO;
    }
    lifetime
        .0
        .clamp(DEFAULT_LIFETIME, MAXIMUM_ALLOCATION_LIFETIME)
}
