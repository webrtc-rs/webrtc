#![warn(rust_2018_idioms)]
#![allow(dead_code)]

//! Async-friendly WebRTC implementation in Rust
//!
//! This crate provides a runtime-agnostic WebRTC implementation built on top of
//! the Sans-I/O [rtc](https://docs.rs/rtc) protocol core.
//!
//! # Runtime Support
//!
//! The library supports multiple async runtimes through feature flags:
//!
//! - `runtime-tokio` (default) - Tokio runtime support
//! - `runtime-smol` - smol runtime support
//!
//! # Example
//!
//! ```no_run
//! // Coming soon: PeerConnection example
//! ```

pub mod peer_connection;
pub mod runtime;
