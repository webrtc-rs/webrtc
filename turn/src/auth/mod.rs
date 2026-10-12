#[cfg(test)]
mod auth_test;

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::prelude::BASE64_STANDARD;
use base64::Engine;
use md5::{Digest, Md5};
use precis_profiles::{precis_core::profile::PrecisFastInvocation, OpaqueString};
use ring::hmac;

use crate::error::*;

pub trait AuthHandler {
    fn auth_handle(&self, username: &str, realm: &str, src_addr: SocketAddr) -> Result<Vec<u8>>;

    /// Password algorithms in preference order; legacy handlers remain compatible.
    fn password_algorithms(&self) -> Vec<u16> {
        vec![1]
    }

    fn auth_handle_algorithm(
        &self,
        username: &str,
        realm: &str,
        src_addr: SocketAddr,
        algorithm: u16,
    ) -> Result<Vec<u8>> {
        if algorithm != 1 {
            return Err(Error::Other("unsupported password algorithm".into()));
        }
        self.auth_handle(username, realm, src_addr)
    }
}

/// `generate_long_term_credentials()` can be used to create credentials valid for `duration` time/
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
    BASE64_STANDARD.encode(password)
}

/// A convenience function to easily generate keys in the format used by [`AuthHandler`].
pub fn generate_auth_key(username: &str, realm: &str, password: &str) -> Vec<u8> {
    let s = format!("{username}:{realm}:{password}");

    let mut h = Md5::new();
    h.update(s.as_bytes());
    h.finalize().as_slice().to_vec()
}

/// Apply the RFC 8489 OpaqueString profile before sending credential strings.
pub fn prepare_opaque_string(value: &str) -> Result<String> {
    OpaqueString::enforce(value)
        .map(|value| value.into_owned())
        .map_err(|error| Error::Other(format!("invalid credential string: {error}")))
}

/// Prepare a REALM value with the length required by RFC 8489 section 14.9.
pub fn prepare_realm(value: &str) -> Result<String> {
    let realm = prepare_opaque_string(value)?;
    if realm.is_empty() || realm.chars().count() >= 128 {
        return Err(Error::Other("invalid realm length".into()));
    }
    Ok(realm)
}

/// Derive a checked long-term key with the registered MD5 (1) or SHA-256 (2) algorithm.
/// The username must already have been prepared for the USERNAME attribute.
pub fn generate_auth_key_algorithm(
    username: &str,
    realm: &str,
    password: &str,
    algorithm: u16,
) -> Result<Vec<u8>> {
    let value = format!(
        "{username}:{}:{}",
        prepare_opaque_string(realm)?,
        prepare_opaque_string(password)?,
    );
    match algorithm {
        1 => Ok(Md5::digest(value.as_bytes()).to_vec()),
        2 => Ok(
            ring::digest::digest(&ring::digest::SHA256, value.as_bytes())
                .as_ref()
                .to_vec(),
        ),
        _ => Err(Error::Other("unsupported password algorithm".into())),
    }
}

/// Static long-term credentials with RFC 8489 password-algorithm negotiation.
pub struct StaticAuthHandler {
    users: BTreeMap<String, String>,
}

impl StaticAuthHandler {
    pub fn new(users: BTreeMap<String, String>) -> Result<Self> {
        let mut normalized = BTreeMap::new();
        for (username, password) in users {
            let username = prepare_opaque_string(&username)?;
            let password = prepare_opaque_string(&password)?;
            if username.len() >= 509 || normalized.insert(username, password).is_some() {
                return Err(Error::Other("duplicate or oversized username".into()));
            }
        }
        Ok(Self { users: normalized })
    }
}

impl AuthHandler for StaticAuthHandler {
    fn password_algorithms(&self) -> Vec<u16> {
        vec![2, 1]
    }

    fn auth_handle_algorithm(
        &self,
        username: &str,
        realm: &str,
        _src_addr: SocketAddr,
        algorithm: u16,
    ) -> Result<Vec<u8>> {
        let password = self
            .users
            .get(username)
            .ok_or_else(|| Error::Other("unknown user".into()))?;
        generate_auth_key_algorithm(username, realm, password, algorithm)
    }

    fn auth_handle(&self, username: &str, realm: &str, src_addr: SocketAddr) -> Result<Vec<u8>> {
        self.auth_handle_algorithm(username, realm, src_addr, 1)
    }
}

pub struct LongTermAuthHandler {
    shared_secret: String,
}

impl AuthHandler for LongTermAuthHandler {
    fn password_algorithms(&self) -> Vec<u16> {
        vec![2, 1]
    }

    fn auth_handle_algorithm(
        &self,
        username: &str,
        realm: &str,
        src_addr: SocketAddr,
        algorithm: u16,
    ) -> Result<Vec<u8>> {
        let legacy = self.auth_handle(username, realm, src_addr)?;
        match algorithm {
            1 => Ok(legacy),
            2 => generate_auth_key_algorithm(
                username,
                realm,
                &long_term_credentials(username, &self.shared_secret),
                2,
            ),
            _ => Err(Error::Other("unsupported password algorithm".into())),
        }
    }

    fn auth_handle(&self, username: &str, realm: &str, src_addr: SocketAddr) -> Result<Vec<u8>> {
        log::trace!("Authentication username={username} realm={realm} src_addr={src_addr}");

        let t = Duration::from_secs(username.parse::<u64>()?);
        if t < SystemTime::now().duration_since(UNIX_EPOCH)? {
            return Err(Error::Other(format!(
                "Expired time-windowed username {username}"
            )));
        }

        let password = long_term_credentials(username, &self.shared_secret);
        generate_auth_key_algorithm(username, realm, &password, 1)
    }
}

impl LongTermAuthHandler {
    /// https://tools.ietf.org/search/rfc5389#section-10.2
    pub fn new(shared_secret: String) -> Self {
        LongTermAuthHandler { shared_secret }
    }
}
