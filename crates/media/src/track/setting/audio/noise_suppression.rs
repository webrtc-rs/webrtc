use std::fmt::Debug;

/// Noise suppression is often desirable on the input signal recorded by the microphone.
///
/// There are cases where it is not needed and it is desirable to turn it off so that
/// the audio is not altered. This allows applications to control this behavior.
///
/// # Specification
/// - <https://www.w3.org/TR/mediacapture-streams/#dfn-noisesuppression>
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum NoiseSuppression {
    Off,
    On,
}

impl From<bool> for NoiseSuppression {
    fn from(boolean: bool) -> Self {
        if boolean {
            Self::On
        } else {
            Self::Off
        }
    }
}

impl Debug for NoiseSuppression {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Off => write!(f, "off"),
            Self::On => write!(f, "on"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from() {
        let scenarios = [(false, NoiseSuppression::Off), (true, NoiseSuppression::On)];

        for (flag, expected) in scenarios {
            let actual = NoiseSuppression::from(flag);
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn debug() {
        let scenarios = [(NoiseSuppression::Off, "off"), (NoiseSuppression::On, "on")];

        for (subject, expected) in scenarios {
            let actual = format!("{:?}", subject);
            assert_eq!(actual, expected);
        }
    }
}
