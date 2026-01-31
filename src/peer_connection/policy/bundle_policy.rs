use std::fmt;

use serde::{Deserialize, Serialize};

/// BundlePolicy affects which media tracks are negotiated if the remote
/// endpoint is not bundle-aware, and what ICE candidates are gathered. If the
/// remote endpoint is bundle-aware, all media tracks and data channels are
/// bundled onto the same transport.
///
/// ## Specifications
///
/// * [W3C]
///
/// [W3C]: https://w3c.github.io/webrtc-pc/#rtcbundlepolicy-enum
#[derive(Default, Debug, PartialEq, Eq, Copy, Clone, Serialize, Deserialize)]
pub enum RTCBundlePolicy {
    #[default]
    Unspecified = 0,

    /// BundlePolicyBalanced indicates to gather ICE candidates for each
    /// media type in use (audio, video, and data). If the remote endpoint is
    /// not bundle-aware, negotiate only one audio and video track on separate
    /// transports.
    #[serde(rename = "balanced")]
    Balanced = 1,

    /// BundlePolicyMaxCompat indicates to gather ICE candidates for each
    /// track. If the remote endpoint is not bundle-aware, negotiate all media
    /// tracks on separate transports.
    #[serde(rename = "max-compat")]
    MaxCompat = 2,

    /// BundlePolicyMaxBundle indicates to gather ICE candidates for only
    /// one track. If the remote endpoint is not bundle-aware, negotiate only
    /// one media track.
    #[serde(rename = "max-bundle")]
    MaxBundle = 3,
}

/// This is done this way because of a linter.
const BUNDLE_POLICY_BALANCED_STR: &str = "balanced";
const BUNDLE_POLICY_MAX_COMPAT_STR: &str = "max-compat";
const BUNDLE_POLICY_MAX_BUNDLE_STR: &str = "max-bundle";

impl From<&str> for RTCBundlePolicy {
    /// NewSchemeType defines a procedure for creating a new SchemeType from a raw
    /// string naming the scheme type.
    fn from(raw: &str) -> Self {
        match raw {
            BUNDLE_POLICY_BALANCED_STR => RTCBundlePolicy::Balanced,
            BUNDLE_POLICY_MAX_COMPAT_STR => RTCBundlePolicy::MaxCompat,
            BUNDLE_POLICY_MAX_BUNDLE_STR => RTCBundlePolicy::MaxBundle,
            _ => RTCBundlePolicy::Unspecified,
        }
    }
}

impl fmt::Display for RTCBundlePolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            RTCBundlePolicy::Balanced => write!(f, "{BUNDLE_POLICY_BALANCED_STR}"),
            RTCBundlePolicy::MaxCompat => write!(f, "{BUNDLE_POLICY_MAX_COMPAT_STR}"),
            RTCBundlePolicy::MaxBundle => write!(f, "{BUNDLE_POLICY_MAX_BUNDLE_STR}"),
            _ => write!(f, "{}", crate::UNSPECIFIED_STR),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_new_bundle_policy() {
        let tests = vec![
            ("Unspecified", RTCBundlePolicy::Unspecified),
            ("balanced", RTCBundlePolicy::Balanced),
            ("max-compat", RTCBundlePolicy::MaxCompat),
            ("max-bundle", RTCBundlePolicy::MaxBundle),
        ];

        for (policy_string, expected_policy) in tests {
            assert_eq!(RTCBundlePolicy::from(policy_string), expected_policy);
        }
    }

    #[test]
    fn test_bundle_policy_string() {
        let tests = vec![
            (RTCBundlePolicy::Unspecified, "Unspecified"),
            (RTCBundlePolicy::Balanced, "balanced"),
            (RTCBundlePolicy::MaxCompat, "max-compat"),
            (RTCBundlePolicy::MaxBundle, "max-bundle"),
        ];

        for (policy, expected_string) in tests {
            assert_eq!(policy.to_string(), expected_string);
        }
    }
}
