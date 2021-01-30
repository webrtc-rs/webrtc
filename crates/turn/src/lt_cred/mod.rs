#[cfg(test)]
mod lt_cred_test;

use crate::server::request::AuthHandler;

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use util::Error;

use ring::hmac;
use std::net::SocketAddr;

// generate_long_term_credentials can be used to create credentials valid for [duration] time
pub fn generate_long_term_credentials(
    shared_secret: &str,
    duration: Duration,
) -> Result<(String, String), Error> {
    let t = SystemTime::now().duration_since(UNIX_EPOCH)? + duration;
    let username = format!("{}", t.as_secs());
    let password = long_term_credentials(&username, shared_secret)?;
    Ok((username, password))
}

fn long_term_credentials(username: &str, shared_secret: &str) -> Result<String, Error> {
    let mac = hmac::Key::new(
        hmac::HMAC_SHA1_FOR_LEGACY_USE_ONLY,
        shared_secret.as_bytes(),
    );
    let password = hmac::sign(&mac, username.as_bytes()).as_ref().to_vec();
    Ok(base64::encode(&password))
}

// generate_auth_key is a convince function to easily generate keys in the format used by AuthHandler
pub fn generate_auth_key(username: &str, realm: &str, password: &str) -> Vec<u8> {
    let h = format!("{}:{}:{}", username, realm, password);
    let digest = md5::compute(h.as_bytes());
    format!("{:x}", digest).as_bytes().to_vec()
}

pub struct LongTermAuthHandler {
    shared_secret: String,
}

impl AuthHandler for LongTermAuthHandler {
    fn auth_handle(
        &self,
        username: &str,
        realm: &str,
        src_addr: SocketAddr,
    ) -> Result<Vec<u8>, Error> {
        log::trace!(
            "Authentication username={} realm={} srcAddr={}\n",
            username,
            realm,
            src_addr
        );

        let t = Duration::from_secs(username.parse::<u64>()?);
        if t < SystemTime::now().duration_since(UNIX_EPOCH)? {
            return Err(Error::new(format!(
                "Expired time-windowed username {}",
                username
            )));
        }

        let password = long_term_credentials(username, &self.shared_secret)?;
        Ok(generate_auth_key(username, realm, &password))
    }
}

impl LongTermAuthHandler {
    // https://tools.ietf.org/search/rfc5389#section-10.2
    pub fn new(shared_secret: String) -> Self {
        LongTermAuthHandler { shared_secret }
    }
}
