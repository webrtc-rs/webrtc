use crate::ice::ice_server::ICEServer;
use crate::policy::ice_transport_policy::ICETransportPolicy;

pub mod ice_gatherer;

/// ICEGatherOptions provides options relating to the gathering of ICE candidates.
pub struct ICEGatherOptions {
    ice_servers: Vec<ICEServer>,
    ice_gather_policy: ICETransportPolicy,
}
