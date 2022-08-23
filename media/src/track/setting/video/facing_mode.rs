use std::{borrow::Cow, fmt::Debug, str::FromStr};

use thiserror::Error;

#[derive(Error, PartialEq, Eq, Debug)]
pub enum FacingModeParsingError {
    #[error("Unknown facing mode: {value}")]
    UnknownValue { value: String },
}

/// The directions that the camera can face, as seen from the user's perspective.
///
/// # Important
/// Note that `getConstraints` may not return exactly the same string for strings not in this enum.
/// This preserves the possibility of using a future version of WebIDL enum for this setting.
///
/// # Specification
/// - <https://www.w3.org/TR/mediacapture-streams/#dfn-facingmode>
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Debug)]
pub enum FacingMode {
    /// The source is facing toward the user (a self-view camera).
    User,

    /// The source is facing away from the user (viewing the environment).
    Environment,

    /// The source is facing to the left of the user.
    Left,

    /// The source is facing to the right of the user.
    Right,
}

impl FromStr for FacingMode {
    type Err = FacingModeParsingError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut string = Cow::from(s);

        if !s.chars().all(|c| c.is_lowercase()) {
            string.to_mut().make_ascii_lowercase();
        }

        match string.as_ref() {
            "user" => Ok(Self::User),
            "environment" => Ok(Self::Environment),
            "left" => Ok(Self::Left),
            "right" => Ok(Self::Right),
            _ => Err(Self::Err::UnknownValue {
                value: s.to_owned(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FACING_MODE: &str = "environment";

    #[test]
    fn from_str_success() {
        let scenarios = [
            ("user", FacingMode::User),
            ("environment", FacingMode::Environment),
            ("left", FacingMode::Left),
            ("right", FacingMode::Right),
        ];

        for (string, expected) in scenarios {
            let actual = FacingMode::from_str(string).unwrap();
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn from_str_failure() {
        let actual = FacingMode::from_str("INVALID");
        let expected = Err(FacingModeParsingError::UnknownValue {
            value: "INVALID".to_owned(),
        });
        assert_eq!(actual, expected);
    }
}
