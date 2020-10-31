use std::string::FromUtf8Error;
use std::time::SystemTimeError;
use std::{fmt, num};

use tokio::sync::mpsc::error::SendError;

use aes_gcm;
use hmac;

use url::ParseError;

#[derive(Debug, Clone, PartialEq)]
pub struct Error {
    message: String,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for Error {
    fn description(&self) -> &str {
        &self.message
    }
}

// Implement std::convert::From for AppError; from io::Error
impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Error {
            message: error.to_string(),
        }
    }
}

impl From<num::ParseIntError> for Error {
    fn from(error: num::ParseIntError) -> Self {
        Error {
            message: error.to_string(),
        }
    }
}

impl From<ParseError> for Error {
    fn from(error: ParseError) -> Self {
        Error {
            message: error.to_string(),
        }
    }
}

impl From<FromUtf8Error> for Error {
    fn from(error: FromUtf8Error) -> Self {
        Error {
            message: error.to_string(),
        }
    }
}

impl From<SystemTimeError> for Error {
    fn from(error: SystemTimeError) -> Self {
        Error {
            message: error.to_string(),
        }
    }
}

impl<T> From<SendError<T>> for Error {
    fn from(error: SendError<T>) -> Self {
        Error {
            message: error.to_string(),
        }
    }
}

impl From<aes_gcm::Error> for Error {
    fn from(error: aes_gcm::Error) -> Self {
        Error {
            message: error.to_string(),
        }
    }
}

impl From<hmac::crypto_mac::InvalidKeyLength> for Error {
    fn from(error: hmac::crypto_mac::InvalidKeyLength) -> Self {
        Error {
            message: error.to_string(),
        }
    }
}

impl From<p256::elliptic_curve::Error> for Error {
    fn from(error: p256::elliptic_curve::Error) -> Self {
        Error {
            message: error.to_string(),
        }
    }
}

impl From<block_modes::InvalidKeyIvLength> for Error {
    fn from(error: block_modes::InvalidKeyIvLength) -> Self {
        Error {
            message: error.to_string(),
        }
    }
}
impl From<block_modes::BlockModeError> for Error {
    fn from(error: block_modes::BlockModeError) -> Self {
        Error {
            message: error.to_string(),
        }
    }
}

impl From<rsa::errors::Error> for Error {
    fn from(error: rsa::errors::Error) -> Self {
        Error {
            message: error.to_string(),
        }
    }
}

impl From<signature::Error> for Error {
    fn from(error: signature::Error) -> Self {
        Error {
            message: error.to_string(),
        }
    }
}

impl From<der_parser::nom::Err<x509_parser::error::X509Error>> for Error {
    fn from(error: der_parser::nom::Err<x509_parser::error::X509Error>) -> Self {
        Error {
            message: error.to_string(),
        }
    }
}

impl From<x509_parser::error::X509Error> for Error {
    fn from(error: x509_parser::error::X509Error) -> Self {
        Error {
            message: error.to_string(),
        }
    }
}

impl Error {
    pub fn new(message: String) -> Self {
        Error { message }
    }
}
