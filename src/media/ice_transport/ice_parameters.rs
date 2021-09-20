use serde::{Deserialize, Serialize};

/// ICEParameters includes the ICE username fragment
/// and password and other ICE-related parameters.
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RTCIceParameters {
    pub username_fragment: String,
    pub password: String,
    pub ice_lite: bool,
}
