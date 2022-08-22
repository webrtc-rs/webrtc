use std::{borrow::Cow, fmt::Debug, str::FromStr};

use thiserror::Error;

#[derive(Error, PartialEq, Debug)]
pub enum ResizeModeParsingError {
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
/// - <https://www.w3.org/TR/mediacapture-streams/#dfn-resizemode>
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Debug)]
pub enum ResizeMode {
    /// This resolution and frame rate is offered by the camera, its driver, or the OS.
    None,

    /// This resolution is down-scaled and/or cropped from a higher camera resolution by the User Agent,
    /// or its frame rate is decimated by the User Agent.
    /// The media MUST NOT be up-scaled, stretched or have fake data
    /// created that did not occur in the input source.
    CropAndScale,
}

impl FromStr for ResizeMode {
    type Err = ResizeModeParsingError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut string = Cow::from(s);

        if !s.chars().all(|c| c.is_lowercase()) {
            string.to_mut().make_ascii_lowercase();
        }

        match string.as_ref() {
            "none" => Ok(Self::None),
            "crop-and-scale" => Ok(Self::CropAndScale),
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
            ("none", ResizeMode::None),
            ("crop-and-scale", ResizeMode::CropAndScale),
        ];

        for (string, expected) in scenarios {
            let actual = ResizeMode::from_str(string).unwrap();
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn from_str_failure() {
        let actual = ResizeMode::from_str("INVALID");
        let expected = Err(ResizeModeParsingError::UnknownValue {
            value: "INVALID".to_owned(),
        });
        assert_eq!(actual, expected);
    }
}
