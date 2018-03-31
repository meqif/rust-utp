use std::error::Error;
use std::fmt;
use std::io::{self, ErrorKind};

#[derive(Debug)]
pub enum ParseError {
    InvalidExtensionLength,
    InvalidPacketLength,
    InvalidPacketType(u8),
    UnsupportedVersion,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.description())
    }
}

impl Error for ParseError {
    fn description(&self) -> &str {
        use self::ParseError::*;
        match *self {
            InvalidExtensionLength => "Invalid extension length (must be a non-zero multiple of 4)",
            InvalidPacketLength => "The packet is too small",
            InvalidPacketType(_) => "Invalid packet type",
            UnsupportedVersion => "Unsupported packet version",
        }
    }
}

impl From<ParseError> for io::Error {
    fn from(error: ParseError) -> io::Error {
        io::Error::new(ErrorKind::Other, error.description())
    }
}
