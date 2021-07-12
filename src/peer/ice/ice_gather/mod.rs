pub mod ice_gatherer;
pub mod ice_gatherer_state;
pub mod ice_gathering_state;

use crate::peer::ice::ice_server::ICEServer;
use crate::peer::policy::ice_transport_policy::ICETransportPolicy;

/// ICEGatherOptions provides options relating to the gathering of ICE candidates.
#[derive(Default, Debug, Clone)]
pub struct ICEGatherOptions {
    pub ice_servers: Vec<ICEServer>,
    pub ice_gather_policy: ICETransportPolicy,
}
