#![warn(rust_2018_idioms)]
#![allow(dead_code)]

use anyhow::Result;
use async_trait::async_trait;

#[cfg(feature = "vnet")]
#[macro_use]
extern crate lazy_static;

#[cfg(target_family = "windows")]
#[macro_use]
extern crate bitflags;

pub mod fixed_big_int;
pub mod replay_detector;

/// KeyingMaterialExporter to extract keying material
#[async_trait]
pub trait KeyingMaterialExporter {
    async fn export_keying_material(
        &self,
        label: &str,
        context: &[u8],
        length: usize,
    ) -> Result<Vec<u8>>;
}

#[cfg(feature = "buffer")]
pub mod buffer;

#[cfg(feature = "conn")]
pub mod conn;

#[cfg(feature = "ifaces")]
pub mod ifaces;

#[cfg(feature = "vnet")]
pub mod vnet;

#[cfg(feature = "marshal")]
pub mod marshal;

#[cfg(feature = "buffer")]
pub use crate::buffer::Buffer;

#[cfg(feature = "conn")]
pub use crate::conn::Conn;

#[cfg(feature = "marshal")]
pub use crate::marshal::{exact_size_buf::ExactSizeBuf, Marshal, MarshalSize, Unmarshal};
