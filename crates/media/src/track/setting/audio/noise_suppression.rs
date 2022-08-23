use std::fmt::Debug;

/// Noise suppression is often desirable on the input signal recorded by the microphone.
///
/// There are cases where it is not needed and it is desirable to turn it off so that
/// the audio is not altered. This allows applications to control this behavior.
///
/// # Specification
/// - <https://www.w3.org/TR/mediacapture-streams/#dfn-noisesuppression>
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Debug)]
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
}
