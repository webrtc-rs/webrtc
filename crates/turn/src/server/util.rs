use crate::proto::lifetime::*;

use std::marker::{Send, Sync};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::SystemTime;

use tokio::time::Duration;

use stun::agent::*;
use stun::message::*;

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
    typ: MessageType,
    mut additional: Vec<Box<dyn Setter>>,
) -> Vec<Box<dyn Setter>> {
    let mut attrs: Vec<Box<dyn Setter>> = vec![Box::new(Message {
        transaction_id,
        typ,
        ..Default::default()
    })];
    attrs.append(&mut additional);
    attrs
}

/*TODO:
pub(crate) fn authenticate_request(r Request, m *stun.Message, callingMethod stun.Method) (stun.MessageIntegrity, bool, error) {
    respondWithNonce := func(responseCode stun.ErrorCode) (stun.MessageIntegrity, bool, error) {
        nonce, err := buildNonce()
        if err != nil {
            return nil, false, err
        }

        // Nonce has already been taken
        if _, keyCollision := r.Nonces.LoadOrStore(nonce, time.Now()); keyCollision {
            return nil, false, errDuplicatedNonce
        }

        return nil, false, buildAndSend(r.Conn, r.SrcAddr, buildMsg(m.TransactionID,
            stun.NewType(callingMethod, stun.ClassErrorResponse),
            &stun.ErrorCodeAttribute{Code: responseCode},
            stun.NewNonce(nonce),
            stun.NewRealm(r.Realm),
        )...)
    }

    if !m.Contains(stun.AttrMessageIntegrity) {
        return respondWithNonce(stun.CodeUnauthorized)
    }

    nonceAttr := &stun.Nonce{}
    usernameAttr := &stun.Username{}
    realmAttr := &stun.Realm{}
    badRequestMsg := buildMsg(m.TransactionID, stun.NewType(callingMethod, stun.ClassErrorResponse), &stun.ErrorCodeAttribute{Code: stun.CodeBadRequest})

    if err := nonceAttr.GetFrom(m); err != nil {
        return nil, false, buildAndSendErr(r.Conn, r.SrcAddr, err, badRequestMsg...)
    }

    // Assert Nonce exists and is not expired
    nonceCreationTime, ok := r.Nonces.Load(string(*nonceAttr))
    if !ok || time.Since(nonceCreationTime.(time.Time)) >= NONCE_LIFETIME {
        r.Nonces.Delete(nonceAttr)
        return respondWithNonce(stun.CodeStaleNonce)
    }

    if err := realmAttr.GetFrom(m); err != nil {
        return nil, false, buildAndSendErr(r.Conn, r.SrcAddr, err, badRequestMsg...)
    } else if err := usernameAttr.GetFrom(m); err != nil {
        return nil, false, buildAndSendErr(r.Conn, r.SrcAddr, err, badRequestMsg...)
    }

    ourKey, ok := r.AuthHandler(usernameAttr.String(), realmAttr.String(), r.SrcAddr)
    if !ok {
        return nil, false, buildAndSendErr(r.Conn, r.SrcAddr, fmt.Errorf("%w %s", errNoSuchUser, usernameAttr.String()), badRequestMsg...)
    }

    if err := stun.MessageIntegrity(ourKey).Check(m); err != nil {
        return nil, false, buildAndSendErr(r.Conn, r.SrcAddr, err, badRequestMsg...)
    }

    return stun.MessageIntegrity(ourKey), true, nil
}
*/

pub(crate) fn allocation_life_time(m: &Message) -> Duration {
    let mut lifetime_duration = DEFAULT_LIFETIME;

    let mut lifetime = Lifetime::default();
    if lifetime.get_from(m).is_ok() && lifetime.0 < MAXIMUM_ALLOCATION_LIFETIME {
        lifetime_duration = lifetime.0;
    }

    lifetime_duration
}
