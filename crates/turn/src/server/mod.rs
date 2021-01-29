#[cfg(test)]
mod server_test;

mod handler;
mod utils;

use crate::allocation::allocation_manager::*;
use crate::errors::*;
use crate::proto::chandata::ChannelData;
use utils::*;

use stun::attributes::*;
use stun::error_code::*;
use stun::integrity::*;
use stun::message::*;
use stun::textattrs::*;

use util::{Conn, Error};

use std::collections::HashMap;
use std::marker::{Send, Sync};
use std::net::SocketAddr;
use std::sync::Arc;

use tokio::sync::Mutex;
use tokio::time::{Duration, Instant};

type AuthHandlerFn = fn(username: String, realm: String, srcAddr: SocketAddr) -> (Vec<u8>, bool);

// Request contains all the state needed to process a single incoming datagram
pub struct Request {
    // Current Request State
    pub conn: Arc<dyn Conn + Send + Sync>,
    pub src_addr: SocketAddr,
    pub buff: Vec<u8>,

    // Server State
    pub allocation_manager: Manager,
    pub nonces: Arc<Mutex<HashMap<String, Instant>>>,

    // User Configuration
    pub auth_handler: AuthHandlerFn,
    pub realm: String,
    pub channel_bind_timeout: Duration,
}

impl Request {
    pub fn new(
        conn: Arc<dyn Conn + Send + Sync>,
        src_addr: SocketAddr,
        allocation_manager: Manager,
        auth_handler: AuthHandlerFn,
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

    // handle_request processes the give Request
    pub async fn handle_request(&mut self) -> Result<(), Error> {
        log::debug!(
            "received {} bytes of udp from {} on {}",
            self.buff.len(),
            self.src_addr,
            self.conn.local_addr()?
        );

        if ChannelData::is_channel_data(&self.buff) {
            self.handle_data_packet().await
        } else {
            self.handle_turn_packet().await
        }
    }

    async fn handle_data_packet(&mut self) -> Result<(), Error> {
        log::debug!("received DataPacket from {}", self.src_addr);
        let mut c = ChannelData {
            raw: self.buff.clone(),
            ..Default::default()
        };
        c.decode()?;
        self.handle_channel_data(&c).await
    }

    async fn handle_turn_packet(&mut self) -> Result<(), Error> {
        log::debug!("handle_turn_packet");
        let mut m = Message {
            raw: self.buff.clone(),
            ..Default::default()
        };
        m.decode()?;

        self.process_message_handler(&m).await
    }

    async fn process_message_handler(&mut self, m: &Message) -> Result<(), Error> {
        if m.typ.class == CLASS_INDICATION {
            match m.typ.method {
                METHOD_SEND => self.handle_send_indication(m).await,
                _ => Err(ERR_UNEXPECTED_CLASS.to_owned()),
            }
        } else if m.typ.class == CLASS_REQUEST {
            match m.typ.method {
                METHOD_ALLOCATE => self.handle_allocate_request(m).await,
                METHOD_REFRESH => self.handle_refresh_request(m).await,
                METHOD_CREATE_PERMISSION => self.handle_create_permission_request(m).await,
                METHOD_CHANNEL_BIND => self.handle_channel_bind_request(m).await,
                METHOD_BINDING => self.handle_binding_request(m).await,
                _ => Err(ERR_UNEXPECTED_CLASS.to_owned()),
            }
        } else {
            Err(ERR_UNEXPECTED_CLASS.to_owned())
        }
    }

    pub(crate) async fn authenticate_request(
        &mut self,
        m: &Message,
        calling_method: Method,
    ) -> Result<MessageIntegrity, Error> {
        if !m.contains(ATTR_MESSAGE_INTEGRITY) {
            self.respond_with_nonce(m, calling_method, CODE_UNAUTHORIZED)
                .await?;
            return Ok(MessageIntegrity::default());
        }

        let mut nonce_attr = Nonce::new(ATTR_NONCE, String::new());
        let mut username_attr = Username::new(ATTR_USERNAME, String::new());
        let mut realm_attr = Realm::new(ATTR_REALM, String::new());
        let bad_request_msg = build_msg(
            m.transaction_id,
            MessageType::new(calling_method, CLASS_ERROR_RESPONSE),
            vec![Box::new(ErrorCodeAttribute {
                code: CODE_BAD_REQUEST,
                reason: vec![],
            })],
        );

        nonce_attr.get_from(m)?;

        let to_be_deleted = {
            // Assert Nonce exists and is not expired
            let mut nonces = self.nonces.lock().await;

            let to_be_deleted = if let Some(nonce_creation_time) = nonces.get(&nonce_attr.text) {
                Instant::now().duration_since(*nonce_creation_time) >= NONCE_LIFETIME
            } else {
                true
            };

            if to_be_deleted {
                nonces.remove(&nonce_attr.text);
            }
            to_be_deleted
        };

        if to_be_deleted {
            self.respond_with_nonce(m, calling_method, CODE_STALE_NONCE)
                .await?;
            return Ok(MessageIntegrity::default());
        }

        realm_attr.get_from(m)?;
        username_attr.get_from(m)?;

        let (our_key, ok) = (self.auth_handler)(
            username_attr.to_string(),
            realm_attr.to_string(),
            self.src_addr,
        );
        if !ok {
            build_and_send_err(
                &self.conn,
                self.src_addr,
                ERR_NO_SUCH_USER.to_owned(),
                &bad_request_msg,
            )
            .await?;
            return Ok(MessageIntegrity::default());
        }

        let mi = MessageIntegrity(our_key);
        if let Err(err) = mi.check(&mut m.clone()) {
            build_and_send_err(&self.conn, self.src_addr, err, &bad_request_msg).await?;
            Ok(MessageIntegrity::default())
        } else {
            Ok(mi)
        }
    }

    async fn respond_with_nonce(
        &mut self,
        m: &Message,
        calling_method: Method,
        response_code: ErrorCode,
    ) -> Result<(), Error> {
        let nonce = build_nonce()?;

        {
            // Nonce has already been taken
            let mut nonces = self.nonces.lock().await;
            if nonces.contains_key(&nonce) {
                return Err(ERR_DUPLICATED_NONCE.to_owned());
            }
            nonces.insert(nonce.clone(), Instant::now());
        }

        build_and_send(
            &self.conn,
            self.src_addr,
            &build_msg(
                m.transaction_id,
                MessageType::new(calling_method, CLASS_ERROR_RESPONSE),
                vec![
                    Box::new(ErrorCodeAttribute {
                        code: response_code,
                        reason: vec![],
                    }),
                    Box::new(Nonce::new(ATTR_NONCE, nonce)),
                    Box::new(Realm::new(ATTR_REALM, self.realm.clone())),
                ],
            ),
        )
        .await
    }
}
