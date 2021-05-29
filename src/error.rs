use std::error::Error;
use std::fmt;
use std::io::{self, ErrorKind};

#[derive(Debug)]
pub enum SocketError {
    ConnectionClosed,
    ConnectionReset,
    ConnectionTimedOut,
    InvalidAddress,
    InvalidReply,
    NotConnected,
    Other(String),
}

impl Error for SocketError {
    fn description(&self) -> &str {
        use self::SocketError::*;
        match *self {
            ConnectionClosed => "The socket is closed",
            ConnectionReset => "Connection reset by remote peer",
            ConnectionTimedOut => "Connection timed out",
            InvalidAddress => "Invalid address",
            InvalidReply => "The remote peer sent an invalid reply",
            NotConnected => "The socket is not connected",
            Other(ref s) => s,
        }
    }
}

impl fmt::Display for SocketError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(self.description())
    }
}

impl From<SocketError> for io::Error {
    fn from(error: SocketError) -> io::Error {
        use self::SocketError::*;
        let kind = match error {
            ConnectionClosed | NotConnected => ErrorKind::NotConnected,
            ConnectionReset => ErrorKind::ConnectionReset,
            ConnectionTimedOut => ErrorKind::TimedOut,
            InvalidAddress => ErrorKind::InvalidInput,
            InvalidReply => ErrorKind::ConnectionRefused,
            Other(_) => ErrorKind::Other,
        };
        io::Error::new(kind, error.description())
    }
}

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
