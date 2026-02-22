#![warn(rust_2018_idioms)]
#![allow(dead_code)]

//! Async-friendly WebRTC implementation in Rust
//!
//! This crate provides an async-friendly runtime-agnostic WebRTC implementation built on top of
//! the Sans-I/O [rtc](https://docs.rs/rtc) protocol core.
//!
//! # Async Runtime Support
//!
//! The library supports multiple async runtimes through feature flags:
//!
//! - `runtime-tokio` (default) - Tokio runtime support
//! - `runtime-smol` - smol runtime support

pub mod data_channel;
pub mod media_stream;
pub mod peer_connection;
pub mod rtp_transceiver;
pub mod runtime;

/// Error and Result types
///
/// Re-exports [`error::Error`] and [`error::Result`] from `rtc-shared` so that
/// callers only need to import from `webrtc::error` rather than reaching into
/// the lower-level crate directly.
pub mod error {
    pub use rtc::shared::error::{Error, Result};
}
