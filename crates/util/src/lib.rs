#![warn(rust_2018_idioms)]
#![allow(dead_code)]

#[macro_use]
extern crate lazy_static;

#[cfg(target_family = "windows")]
#[macro_use]
extern crate bitflags;

pub mod buffer;
pub mod conn;
pub mod error;
pub mod fixed_big_int;
pub mod ifaces;
pub mod replay_detector;
pub mod vnet;

pub use crate::buffer::Buffer;
pub use crate::conn::Conn;
pub use crate::error::Error;
