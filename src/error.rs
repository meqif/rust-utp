use std::error::Error;
use std::fmt;
use std::io::{self, ErrorKind};

#[derive(Debug)]
pub enum SocketError {
    ConnectionClosed,
    ConnectionReset,
    ConnectionTimedOut,
    InvalidAddress,
    InvalidPacket,
    InvalidReply,
    NotConnected,
}

impl Error for SocketError {
    fn description(&self) -> &str {
        use self::SocketError::*;
        match *self {
            ConnectionClosed   => "The socket is closed",
            ConnectionReset    => "Connection reset by remote peer",
            ConnectionTimedOut => "Connection timed out",
            InvalidAddress     => "Invalid address",
            InvalidPacket      => "Error parsing packet",
            InvalidReply       => "The remote peer sent an invalid reply",
            NotConnected       => "The socket is not connected",
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
            ConnectionClosed |
            NotConnected       => ErrorKind::NotConnected,
            ConnectionReset    => ErrorKind::ConnectionReset,
            ConnectionTimedOut => ErrorKind::TimedOut,
            InvalidAddress     => ErrorKind::InvalidInput,
            InvalidReply       => ErrorKind::ConnectionRefused,
            InvalidPacket      => ErrorKind::Other,
            Unimplemented(_)   => ErrorKind::Other,
            Other(_)           => ErrorKind::Other,
        };
        io::Error::new(kind, error.description())
    }
}
