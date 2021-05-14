use thiserror::Error;

#[derive(Error, Eq, PartialEq, Clone, Debug)]
pub enum ChannelTypeError {
    // Marshal buffer was too short
    UnexpectedEndOfBuffer { expected: usize, actual: usize },

    // Remote requested a channel type that we don't support
    InvalidChannelType { invalid_type: u8 },
}

impl std::fmt::Display for ChannelTypeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnexpectedEndOfBuffer { expected, actual } => {
                writeln!(
                    f,
                    "Marshal buffer was too short: (expected: {:?}, actual: {:?})",
                    expected, actual
                )
            }
            Self::InvalidChannelType { invalid_type } => {
                writeln!(f, "Invalid channel type: {:?}", invalid_type)
            }
        }
    }
}

#[derive(Error, Eq, PartialEq, Clone, Debug)]
pub enum DataChannelAckError {}

#[derive(Error, Eq, PartialEq, Clone, Debug)]
pub enum MessageTypeError {
    // Marshal buffer was too short
    UnexpectedEndOfBuffer { expected: usize, actual: usize },

    // DataChannel message has a type we don't support
    InvalidMessageType { invalid_type: u8 },
}

impl std::fmt::Display for MessageTypeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnexpectedEndOfBuffer { expected, actual } => {
                writeln!(
                    f,
                    "Marshal buffer was too short: (expected: {:?}, actual: {:?})",
                    expected, actual
                )
            }
            Self::InvalidMessageType { invalid_type } => {
                writeln!(f, "Invalid message type: {:?}", invalid_type)
            }
        }
    }
}
