/// AnswerOptions structure describes the options used to control the answer
/// creation process.
#[derive(Default, Debug, PartialEq, Copy, Clone)]
pub struct RTCAnswerOptions {
    /// voice_activity_detection allows the application to provide information
    /// about whether it wishes voice detection feature to be enabled or disabled.
    pub voice_activity_detection: bool,
}

/// OfferOptions structure describes the options used to control the offer
/// creation process
#[derive(Default, Debug, PartialEq, Copy, Clone)]
pub struct RTCOfferOptions {
    /// voice_activity_detection allows the application to provide information
    /// about whether it wishes voice detection feature to be enabled or disabled.
    pub voice_activity_detection: bool,

    /// ice_restart forces the underlying ice gathering process to be restarted.
    /// When this value is true, the generated description will have ICE
    /// credentials that are different from the current credentials
    pub ice_restart: bool,
}
