pub mod ice_gatherer;
pub mod ice_gathering_state;

use crate::ice::ice_server::ICEServer;
use crate::policy::ice_transport_policy::ICETransportPolicy;

/// ICEGatherOptions provides options relating to the gathering of ICE candidates.
pub struct ICEGatherOptions {
    ice_servers: Vec<ICEServer>,
    ice_gather_policy: ICETransportPolicy,
}
