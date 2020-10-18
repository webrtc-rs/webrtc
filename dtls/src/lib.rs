#![warn(rust_2018_idioms)]
#![allow(dead_code)]

#[macro_use]
extern crate lazy_static;

pub mod alert;
pub mod application_data;
pub mod change_cipher_spec;
pub mod compression_methods;
pub mod content;
pub mod curve;
pub mod errors;
pub mod extension;
pub mod handshake;
pub mod record_layer;
pub mod signature_hash_algorithm;
