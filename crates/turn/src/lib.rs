#![warn(rust_2018_idioms)]
#![allow(dead_code)]

#[macro_use]
extern crate lazy_static;

pub mod allocation;
pub mod client;
pub mod errors;
pub mod proto;
pub mod relay_address_generator;
