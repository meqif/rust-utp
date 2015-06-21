#[cfg(unix)]
extern crate nix;
#[cfg(windows)]
extern crate libc;

use std::io::Result;
use std::net::{UdpSocket, SocketAddr};

/// A trait to make time-limited reads from socket-like objects.
pub trait WithReadTimeout {
    /// Receives data from the object, blocking for at most the specified number of milliseconds.
    /// On success, returns the number of bytes read and the address from whence the data came.  If
    /// the timeout expires, it returns `ErrorKind::WouldBlock`.
    fn recv_timeout(&mut self, &mut [u8], i64) -> Result<(usize, SocketAddr)>;
}

impl WithReadTimeout for UdpSocket {
    #[cfg(unix)]
    fn recv_timeout(&mut self, buf: &mut [u8], timeout: i64) -> Result<(usize, SocketAddr)> {
        use nix::sys::socket::{SockLevel, sockopt, setsockopt};
        use nix::sys::time::TimeVal;
        use std::os::unix::io::AsRawFd;

        setsockopt(self.as_raw_fd(),
                   SockLevel::Socket,
                   sockopt::ReceiveTimeout,
                   &TimeVal::milliseconds(timeout)).unwrap();
        self.recv_from(buf)
    }

    #[cfg(windows)]
    /// TODO: Set timeout on socket
    fn recv_timeout(&mut self, buf: &mut [u8], _timeout: i64) -> Result<(usize, SocketAddr)> {
        self.recv_from(buf)
    }
}
