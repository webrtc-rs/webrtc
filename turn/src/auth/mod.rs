#[cfg(test)]
mod auth_test;

use crate::error::*;

use std::net::SocketAddr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use md5::{Digest, Md5};
use ring::hmac;

pub trait AuthHandler {
    fn auth_handle(&self, username: &str, realm: &str, src_addr: SocketAddr) -> Result<Vec<u8>>;
}

// generate_long_term_credentials can be used to create credentials valid for [duration] time
pub fn generate_long_term_credentials(
    shared_secret: &str,
    duration: Duration,
) -> Result<(String, String)> {
    let t = SystemTime::now().duration_since(UNIX_EPOCH)? + duration;
    let username = format!("{}", t.as_secs());
    let password = long_term_credentials(&username, shared_secret);
    Ok((username, password))
}

fn long_term_credentials(username: &str, shared_secret: &str) -> String {
    let mac = hmac::Key::new(
        hmac::HMAC_SHA1_FOR_LEGACY_USE_ONLY,
        shared_secret.as_bytes(),
    );
    let password = hmac::sign(&mac, username.as_bytes()).as_ref().to_vec();
    base64::encode(password)
}

// generate_auth_key is a convenience function to easily generate keys in the format used by AuthHandler
pub fn generate_auth_key(username: &str, realm: &str, password: &str) -> Vec<u8> {
    let s = format!("{username}:{realm}:{password}");

    let mut h = Md5::new();
    h.update(s.as_bytes());
    h.finalize().as_slice().to_vec()
}

pub struct LongTermAuthHandler {
    shared_secret: String,
}

impl AuthHandler for LongTermAuthHandler {
    fn auth_handle(&self, username: &str, realm: &str, src_addr: SocketAddr) -> Result<Vec<u8>> {
        log::trace!(
            "Authentication username={} realm={} src_addr={}",
            username,
            realm,
            src_addr
        );

        let t = Duration::from_secs(username.parse::<u64>()?);
        if t < SystemTime::now().duration_since(UNIX_EPOCH)? {
            return Err(Error::Other(format!(
                "Expired time-windowed username {username}"
            )));
        }

        let password = long_term_credentials(username, &self.shared_secret);
        Ok(generate_auth_key(username, realm, &password))
    }
}

impl LongTermAuthHandler {
    // https://tools.ietf.org/search/rfc5389#section-10.2
    pub fn new(shared_secret: String) -> Self {
        LongTermAuthHandler { shared_secret }
    }
}
