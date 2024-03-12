/// Describes the options used to control the answer creation process.
#[derive(Default, Debug, PartialEq, Eq, Copy, Clone)]
pub struct RTCAnswerOptions {
    /// Allows the application to provide information
    /// about whether it wishes voice detection feature to be enabled or disabled.
    pub voice_activity_detection: bool,
}

/// Describes the options used to control the offer creation process
#[derive(Default, Debug, PartialEq, Eq, Copy, Clone)]
pub struct RTCOfferOptions {
    /// Allows the application to provide information
    /// about whether it wishes voice detection feature to be enabled or disabled.
    pub voice_activity_detection: bool,

    /// When this value is `true`, the generated description will have ICE
    /// credentials that are different from the current credentials. This
    /// will result in the ICE connection being restarted when the offer is
    /// applied.
    ///
    /// When this value is `false`, the generated description will have the
    /// same ICE credentials as the current offer. This is the default.
    pub ice_restart: bool,
}
