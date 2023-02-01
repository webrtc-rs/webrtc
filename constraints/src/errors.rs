//! Errors, as defined in the ["Media Capture and Streams"][mediacapture_streams] spec.
//!
//! [mediacapture_streams]: https://www.w3.org/TR/mediacapture-streams/

use std::collections::HashMap;

use crate::{
    algorithms::{ConstraintFailureInfo, SettingFitnessDistanceErrorKind},
    MediaTrackProperty,
};

/// An error indicating one or more over-constrained settings.
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct OverconstrainedError {
    /// The offending constraint's name.
    pub constraint: MediaTrackProperty,
    /// An error message, or `None` if exposure-mode was `Protected`.
    pub message: Option<String>,
}

impl Default for OverconstrainedError {
    fn default() -> Self {
        Self {
            constraint: MediaTrackProperty::from(""),
            message: Default::default(),
        }
    }
}

impl std::fmt::Display for OverconstrainedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Overconstrained property {:?}", self.constraint)?;
        if let Some(message) = self.message.as_ref() {
            write!(f, ": {message}")?;
        }
        Ok(())
    }
}

impl std::error::Error for OverconstrainedError {}

impl OverconstrainedError {
    pub(super) fn exposing_device_information(
        failed_constraints: HashMap<MediaTrackProperty, ConstraintFailureInfo>,
    ) -> Self {
        let failed_constraint = failed_constraints
            .into_iter()
            .max_by_key(|(_, failure_info)| failure_info.failures);

        let (constraint, failure_info) =
            failed_constraint.expect("Empty candidates implies non-empty failed constraints");

        struct Violation {
            constraint: String,
            settings: Vec<String>,
        }
        let mut violators_by_kind: HashMap<SettingFitnessDistanceErrorKind, Violation> =
            HashMap::default();

        for error in failure_info.errors {
            let violation = violators_by_kind.entry(error.kind).or_insert(Violation {
                constraint: error.constraint.clone(),
                settings: vec![],
            });
            assert_eq!(violation.constraint, error.constraint);
            if let Some(setting) = error.setting {
                violation.settings.push(setting.clone());
            }
        }

        let formatted_reasons: Vec<_> = violators_by_kind
            .into_iter()
            .map(|(kind, violation)| {
                let kind_str = match kind {
                    SettingFitnessDistanceErrorKind::Missing => "missing",
                    SettingFitnessDistanceErrorKind::Mismatch => "a mismatch",
                    SettingFitnessDistanceErrorKind::TooSmall => "too small",
                    SettingFitnessDistanceErrorKind::TooLarge => "too large",
                };

                let mut settings = violation.settings;

                if settings.is_empty() {
                    return format!("{} (does not satisfy {})", kind_str, violation.constraint);
                }

                settings.sort();

                format!(
                    "{} ([{}] do not satisfy {})",
                    kind_str,
                    settings.join(", "),
                    violation.constraint
                )
            })
            .collect();

        let formatted_reason = match &formatted_reasons[..] {
            [] => unreachable!(),
            [reason] => reason.clone(),
            [reasons @ .., reason] => {
                let reasons = reasons.join(", ");
                format!("either {reasons}, or {reason}")
            }
        };
        let message = Some(format!("Setting was {formatted_reason}."));

        Self {
            constraint,
            message,
        }
    }
}
