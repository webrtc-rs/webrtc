#![warn(rust_2018_idioms)]
#![allow(dead_code)]

#[macro_use]
extern crate lazy_static;

pub mod addr;
pub mod agent;
pub mod attributes;
mod checks;
pub mod client;
pub mod error_code;
pub mod errors;
pub mod fingerprint;
pub mod integrity;
pub mod message;
pub mod textattrs;
pub mod uattrs;
pub mod uri;
pub mod xoraddr;
