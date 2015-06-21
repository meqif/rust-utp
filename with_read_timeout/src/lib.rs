#[cfg(unix)]
extern crate nix;
#[cfg(windows)]
extern crate libc;

use std::io::Result;
use std::net::{UdpSocket, SocketAddr};

pub trait WithReadTimeout {
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
    fn recv_timeout(&mut self, buf: &mut [u8], _timeout: i64) -> Result<(usize, SocketAddr)> {
        self.recv_from(buf)
    }
}
