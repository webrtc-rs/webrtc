use super::*;
use crate::errors::*;
use crate::proto::lifetime::*;

use std::marker::{Send, Sync};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::SystemTime;

use tokio::time::Duration;

use stun::agent::*;
use stun::attributes::*;
use stun::error_code::*;
use stun::integrity::*;
use stun::message::*;
use stun::textattrs::*;

use util::{Conn, Error};

pub(crate) const MAXIMUM_ALLOCATION_LIFETIME: Duration = Duration::from_secs(3600); // https://tools.ietf.org/html/rfc5766#section-6.2 defines 3600 seconds recommendation
pub(crate) const NONCE_LIFETIME: Duration = Duration::from_secs(3600); // https://tools.ietf.org/html/rfc5766#section-4

pub(crate) fn rand_seq(n: usize) -> String {
    let letters = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ".as_bytes();
    let mut buf = vec![0u8; n];
    for b in &mut buf {
        *b = letters[rand::random::<usize>() % letters.len()];
    }
    if let Ok(s) = String::from_utf8(buf) {
        s
    } else {
        String::new()
    }
}

pub(crate) fn build_nonce() -> Result<String, Error> {
    /* #nosec */
    let mut h = String::new();
    h.push_str(
        format!(
            "{}",
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)?
                .as_nanos()
        )
        .as_str(),
    );
    h.push_str(format!("{}", rand::random::<u64>()).as_str());
    let digest = md5::compute(h.as_bytes());
    Ok(format!("{:x}", digest))
}

pub(crate) async fn build_and_send(
    conn: &Arc<dyn Conn + Send + Sync>,
    dst: SocketAddr,
    attrs: &[Box<dyn Setter>],
) -> Result<(), Error> {
    let mut msg = Message::new();
    msg.build(attrs)?;
    let _ = conn.send_to(&msg.raw, dst).await?;
    Ok(())
}

// Send a STUN packet and return the original error to the caller
pub(crate) async fn build_and_send_err(
    conn: &Arc<dyn Conn + Send + Sync>,
    dst: SocketAddr,
    err: Error,
    attrs: &[Box<dyn Setter>],
) -> Result<(), Error> {
    if let Err(send_err) = build_and_send(conn, dst, attrs).await {
        Err(send_err)
    } else {
        Err(err)
    }
}

pub(crate) fn build_msg(
    transaction_id: TransactionId,
    msg_type: MessageType,
    mut additional: Vec<Box<dyn Setter>>,
) -> Vec<Box<dyn Setter>> {
    let mut attrs: Vec<Box<dyn Setter>> = vec![
        Box::new(Message {
            transaction_id,
            ..Default::default()
        }),
        Box::new(msg_type),
    ];
    attrs.append(&mut additional);
    attrs
}

async fn respond_with_nonce(
    r: &mut Request,
    m: &Message,
    calling_method: Method,
    response_code: ErrorCode,
) -> Result<(), Error> {
    let nonce = build_nonce()?;

    {
        // Nonce has already been taken
        let mut nonces = r.nonces.lock().await;
        if nonces.contains_key(&nonce) {
            return Err(ERR_DUPLICATED_NONCE.to_owned());
        }
        nonces.insert(nonce.clone(), Instant::now());
    }

    build_and_send(
        &r.conn,
        r.src_addr,
        &build_msg(
            m.transaction_id,
            MessageType::new(calling_method, CLASS_ERROR_RESPONSE),
            vec![
                Box::new(ErrorCodeAttribute {
                    code: response_code,
                    reason: vec![],
                }),
                Box::new(Nonce::new(ATTR_NONCE, nonce)),
                Box::new(Realm::new(ATTR_REALM, r.realm.clone())),
            ],
        ),
    )
    .await
}

pub(crate) async fn authenticate_request(
    r: &mut Request,
    m: &Message,
    calling_method: Method,
) -> Result<MessageIntegrity, Error> {
    if !m.contains(ATTR_MESSAGE_INTEGRITY) {
        respond_with_nonce(r, m, calling_method, CODE_UNAUTHORIZED).await?;
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
        let mut nonces = r.nonces.lock().await;

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
        respond_with_nonce(r, m, calling_method, CODE_STALE_NONCE).await?;
        return Ok(MessageIntegrity::default());
    }

    realm_attr.get_from(m)?;
    username_attr.get_from(m)?;

    let (our_key, ok) = (r.auth_handler)(
        username_attr.to_string(),
        realm_attr.to_string(),
        r.src_addr,
    );
    if !ok {
        build_and_send_err(
            &r.conn,
            r.src_addr,
            ERR_NO_SUCH_USER.to_owned(),
            &bad_request_msg,
        )
        .await?;
        return Ok(MessageIntegrity::default());
    }

    let mi = MessageIntegrity(our_key);
    if let Err(err) = mi.check(&mut m.clone()) {
        build_and_send_err(&r.conn, r.src_addr, err, &bad_request_msg).await?;
        Ok(MessageIntegrity::default())
    } else {
        Ok(mi)
    }
}

pub(crate) fn allocation_life_time(m: &Message) -> Duration {
    let mut lifetime_duration = DEFAULT_LIFETIME;

    let mut lifetime = Lifetime::default();
    if lifetime.get_from(m).is_ok() && lifetime.0 < MAXIMUM_ALLOCATION_LIFETIME {
        lifetime_duration = lifetime.0;
    }

    lifetime_duration
}
