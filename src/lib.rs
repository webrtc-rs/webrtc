#![doc(
    html_logo_url = "https://raw.githubusercontent.com/webrtc-rs/webrtc-rs.github.io/master/res/webrtc.rs.png"
)]
#![warn(rust_2018_idioms)]
#![warn(missing_docs)]
#![allow(dead_code)]

//! # Async WebRTC
//!
//! `webrtc` is an async-friendly, runtime-agnostic WebRTC implementation in Rust.
//! It is built as a thin async layer on top of the battle-tested Sans-I/O [`rtc`](https://docs.rs/rtc) protocol core.
//!
//! ## Architecture
//!
//! The crate separates protocol state from I/O using a driver-based architecture:
//!
//! *   **`PeerConnection`**: The user-facing API handle. All operations (e.g., creating offers, adding tracks,
//!     creating data channels) are asynchronous and communicate with a background driver.
//! *   **`PeerConnectionDriver`**: An internal background event loop spawned automatically. It coordinates network
//!     sockets (UDP/TCP), handles timeouts, drives the underlying Sans-I/O `rtc` core, and dispatches events.
//! *   **`Runtime`**: A trait abstracting async operations (timers, spawning, sockets). This allows the crate to
//!     be completely runtime-agnostic.
//!
//! ## Async Runtime Support
//!
//! The library supports multiple async runtimes through Cargo features:
//!
//! *   **`runtime-tokio` (default)**: Integrates with the Tokio async runtime.
//! *   **`runtime-smol`**: Integrates with the smol async runtime.
//!
//! ## Quick Start
//!
//! Below is a simple example showing how to build a [`PeerConnection`](crate::peer_connection::PeerConnection)
//! and initiate an SDP offer.
//!
//! ```no_run
//! use webrtc::peer_connection::{
//!     PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler,
//!     RTCConfigurationBuilder, RTCIceServer, RTCPeerConnectionIceEvent,
//! };
//! use std::sync::Arc;
//!
//! // 1. Implement the PeerConnectionEventHandler trait to handle events
//! #[derive(Clone)]
//! struct MyHandler;
//!
//! #[async_trait::async_trait]
//! impl PeerConnectionEventHandler for MyHandler {
//!     async fn on_ice_candidate(&self, event: RTCPeerConnectionIceEvent) {
//!         println!("New local ICE candidate gathered: {}", event.candidate);
//!     }
//! }
//!
//! # #[cfg(feature = "runtime-tokio")]
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // 2. Configure the peer connection
//!     let config = RTCConfigurationBuilder::default()
//!         .with_ice_servers(vec![RTCIceServer {
//!             urls: vec!["stun:stun.l.google.com:19302".to_owned()],
//!             ..Default::default()
//!         }])
//!         .build();
//!
//!     // 3. Build the PeerConnection
//!     let pc = PeerConnectionBuilder::new()
//!         .with_configuration(config)
//!         .with_handler(Arc::new(MyHandler))
//!         .with_udp_addrs(vec!["0.0.0.0:0"])
//!         .build()
//!         .await?;
//!
//!     // 4. Create an SDP offer and set it as local description
//!     let offer = pc.create_offer(None).await?;
//!     pc.set_local_description(offer).await?;
//!     
//!     println!("Local description set successfully!");
//!     Ok(())
//! }
//! # #[cfg(not(feature = "runtime-tokio"))]
//! # fn main() {}
//! ```

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
