use std::fmt::Debug;

/// When one or more audio streams is being played in the processes of various microphones,
/// it is often desirable to attempt to remove all the sound being played from the input signals
/// recorded by the microphones. This is referred to as echo cancellation.
///
/// There are cases where it is not needed and it is desirable to turn it off
/// so that no audio artifacts are introduced. This allows applications to control this behavior.
///
/// # Specification
/// - <https://www.w3.org/TR/mediacapture-streams/#dfn-echocancellation>
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Debug)]
pub enum EchoCancellation {
    Off,
    On,
}

impl From<bool> for EchoCancellation {
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
        let scenarios = [(false, EchoCancellation::Off), (true, EchoCancellation::On)];

        for (flag, expected) in scenarios {
            let actual = EchoCancellation::from(flag);
            assert_eq!(actual, expected);
        }
    }
}
