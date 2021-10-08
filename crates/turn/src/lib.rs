#![warn(rust_2018_idioms)]
#![allow(dead_code)]
#![recursion_limit = "256"]

pub mod allocation;
pub mod auth;
pub mod client;
mod error;
pub mod proto;
pub mod relay;
pub mod server;

pub use error::Error;
