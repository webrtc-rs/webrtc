#![warn(rust_2018_idioms)]
#![allow(dead_code)]
#![recursion_limit = "256"]

#[macro_use]
extern crate lazy_static;

pub mod allocation;
pub mod auth;
pub mod client;
pub mod errors;
pub mod proto;
pub mod relay;
pub mod server;
