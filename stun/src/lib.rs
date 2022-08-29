#![warn(rust_2018_idioms)]
#![allow(dead_code)]

#[macro_use]
extern crate lazy_static;

pub mod addr;
pub mod agent;
pub mod attributes;
pub mod checks;
pub mod client;
mod error;
pub mod error_code;
pub mod fingerprint;
pub mod integrity;
pub mod message;
pub mod textattrs;
pub mod uattrs;
pub mod uri;
pub mod xoraddr;

// IANA assigned ports for "stun" protocol.
pub const DEFAULT_PORT: u16 = 3478;
pub const DEFAULT_TLS_PORT: u16 = 5349;

pub use error::Error;
