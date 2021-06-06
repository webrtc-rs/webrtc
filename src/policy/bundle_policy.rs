use std::fmt;

/// BundlePolicy affects which media tracks are negotiated if the remote
/// endpoint is not bundle-aware, and what ICE candidates are gathered. If the
/// remote endpoint is bundle-aware, all media tracks and data channels are
/// bundled onto the same transport.
#[derive(Debug, PartialEq, Copy, Clone)]
pub enum BundlePolicy {
    Unspecified,

    /// BundlePolicyBalanced indicates to gather ICE candidates for each
    /// media type in use (audio, video, and data). If the remote endpoint is
    /// not bundle-aware, negotiate only one audio and video track on separate
    /// transports.
    Balanced,

    /// BundlePolicyMaxCompat indicates to gather ICE candidates for each
    /// track. If the remote endpoint is not bundle-aware, negotiate all media
    // tracks on separate transports.
    MaxCompat,

    /// BundlePolicyMaxBundle indicates to gather ICE candidates for only
    /// one track. If the remote endpoint is not bundle-aware, negotiate only
    /// one media track.
    MaxBundle,
}

/// This is done this way because of a linter.
const BUNDLE_POLICY_BALANCED_STR: &str = "balanced";
const BUNDLE_POLICY_MAX_COMPAT_STR: &str = "max-compat";
const BUNDLE_POLICY_MAX_BUNDLE_STR: &str = "max-bundle";

impl Default for BundlePolicy {
    fn default() -> Self {
        BundlePolicy::Unspecified
    }
}

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
            _ => write!(f, "Unspecified BundlePolicy"),
        }
    }
}

/*
impl BundlePolicy{
    /// unmarshal_json parses the JSON-encoded data and stores the result
    pub fn unmarshal_json(b []byte) error {
        var val string
        if err := json.Unmarshal(b, &val); err != nil {
            return err
        }

        *t = newBundlePolicy(val)
        return nil
    }

    /// marshal_json returns the JSON encoding
    pub fn marshal_json() ([]byte, error) {
        return json.Marshal(t.String())
    }
}*/
