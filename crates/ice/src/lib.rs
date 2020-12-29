#![warn(rust_2018_idioms)]
#![allow(dead_code)]

#[macro_use]
extern crate lazy_static;

pub mod agent;
pub mod candidate;
pub mod errors;
pub mod external_ip_mapper;
pub mod mdns;
pub mod network_type;
pub mod priority;
pub mod state;
pub mod tcp_type;
pub mod url;
pub mod use_candidate;
mod util;
