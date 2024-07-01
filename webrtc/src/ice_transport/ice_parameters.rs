use serde::{Deserialize, Serialize};

/// ICEParameters includes the ICE username fragment
/// and password and other ICE-related parameters.
///
/// ## Specifications
///
/// * [MDN]
/// * [W3C]
///
/// [MDN]: https://developer.mozilla.org/en-US/docs/Web/API/RTCIceParameters
/// [W3C]: https://w3c.github.io/webrtc-pc/#rtciceparameters
#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RTCIceParameters {
    pub username_fragment: String,
    pub password: String,
    pub ice_lite: bool,
}
