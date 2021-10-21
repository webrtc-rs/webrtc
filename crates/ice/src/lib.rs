#![warn(rust_2018_idioms)]
#![allow(dead_code)]

use std::sync::Arc;

pub mod agent;
pub mod candidate;
pub mod control;
mod error;
pub mod external_ip_mapper;
pub mod mdns;
pub mod network_type;
pub mod priority;
pub mod rand;
pub mod state;
pub mod stats;
pub mod tcp_type;
pub mod udp_mux;
pub mod url;
pub mod use_candidate;
mod util;

#[derive(Default, Clone)]
pub struct EphemeralUDP {
    pub port_min: u16,
    pub port_max: u16,
}

/// Configuration for the underlying UDP network stack.
/// There are two ways to configure this Ephemeral and Muxed.
///
/// **Ephemeral mode**
///
/// In Ephemeral mode sockets are created and bound to random ports during ICE
/// gathering. The ports to use can be restricted by setting [`EphemeralUDP::port_min`] and
/// [`EphemeralEphemeralUDP::port_max`] in which case only ports in this range will be used.
///
/// **Muxed**
///
/// In muxed mode a single UDP socket is used and all connections are muxed over this single socket.
///
#[derive(Clone)]
pub enum UDPNetwork {
    Ephemeral(EphemeralUDP),
    Muxed(Arc<dyn udp_mux::UDPMux + Send + Sync>),
}

impl Default for UDPNetwork {
    fn default() -> Self {
        Self::Ephemeral(Default::default())
    }
}

impl UDPNetwork {
    fn is_ephemeral(&self) -> bool {
        matches!(self, Self::Ephemeral(_))
    }

    fn is_muxed(&self) -> bool {
        matches!(self, Self::Muxed(_))
    }
}

pub use error::Error;
