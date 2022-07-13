//! Errors, as defined in the ["Media Capture and Streams"][mediacapture_streams] spec.
//!
//! [mediacapture_streams]: https://www.w3.org/TR/mediacapture-streams/

#[derive(Clone, Default, Eq, PartialEq, Debug)]
pub struct OverconstrainedError {
    /// The offending constraint's name.
    pub constraint: String,
    /// Error message.
    pub message: Option<String>,
}

impl std::fmt::Display for OverconstrainedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Overconstrained property {:?}", self.constraint)?;
        if let Some(message) = self.message.as_ref() {
            write!(f, ": {}", message)?;
        }
        Ok(())
    }
}

impl std::error::Error for OverconstrainedError {}
