use crate::peer_connection::ice::ice_server::RTCIceServer;
use crate::peer_connection::policy::ice_transport_policy::RTCIceTransportPolicy;

pub mod ice_gatherer;
pub mod ice_gatherer_state;
pub mod ice_gathering_state;

/// ICEGatherOptions provides options relating to the gathering of ICE candidates.
#[derive(Default, Debug, Clone)]
pub struct RTCIceGatherOptions {
    pub ice_servers: Vec<RTCIceServer>,
    pub ice_gather_policy: RTCIceTransportPolicy,
}
