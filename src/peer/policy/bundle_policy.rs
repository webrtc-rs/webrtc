use serde::{Deserialize, Serialize};
use std::fmt;

/// BundlePolicy affects which media tracks are negotiated if the remote
/// endpoint is not bundle-aware, and what ICE candidates are gathered. If the
/// remote endpoint is bundle-aware, all media tracks and data channels are
/// bundled onto the same transport.
#[derive(Debug, PartialEq, Copy, Clone, Serialize, Deserialize)]
pub enum BundlePolicy {
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

impl Default for BundlePolicy {
    fn default() -> Self {
        BundlePolicy::Unspecified
    }
}

/// This is done this way because of a linter.
const BUNDLE_POLICY_BALANCED_STR: &str = "balanced";
const BUNDLE_POLICY_MAX_COMPAT_STR: &str = "max-compat";
const BUNDLE_POLICY_MAX_BUNDLE_STR: &str = "max-bundle";

impl From<&str> for BundlePolicy {
    /// NewSchemeType defines a procedure for creating a new SchemeType from a raw
    /// string naming the scheme type.
    fn from(raw: &str) -> Self {
        match raw {
            BUNDLE_POLICY_BALANCED_STR => BundlePolicy::Balanced,
            BUNDLE_POLICY_MAX_COMPAT_STR => BundlePolicy::MaxCompat,
            BUNDLE_POLICY_MAX_BUNDLE_STR => BundlePolicy::MaxBundle,
            _ => BundlePolicy::Unspecified,
        }
    }
}

impl fmt::Display for BundlePolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            BundlePolicy::Balanced => write!(f, "{}", BUNDLE_POLICY_BALANCED_STR),
            BundlePolicy::MaxCompat => write!(f, "{}", BUNDLE_POLICY_MAX_COMPAT_STR),
            BundlePolicy::MaxBundle => write!(f, "{}", BUNDLE_POLICY_MAX_BUNDLE_STR),
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
            ("Unspecified", BundlePolicy::Unspecified),
            ("balanced", BundlePolicy::Balanced),
            ("max-compat", BundlePolicy::MaxCompat),
            ("max-bundle", BundlePolicy::MaxBundle),
        ];

        for (policy_string, expected_policy) in tests {
            assert_eq!(expected_policy, BundlePolicy::from(policy_string));
        }
    }

    #[test]
    fn test_bundle_policy_string() {
        let tests = vec![
            (BundlePolicy::Unspecified, "Unspecified"),
            (BundlePolicy::Balanced, "balanced"),
            (BundlePolicy::MaxCompat, "max-compat"),
            (BundlePolicy::MaxBundle, "max-bundle"),
        ];

        for (policy, expected_string) in tests {
            assert_eq!(expected_string, policy.to_string());
        }
    }
}
