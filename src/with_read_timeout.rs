use std::io::{Error, ErrorKind, Result};
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
        use nix::sys::socket::{sockopt, setsockopt};
        use nix::sys::time::TimeVal;
        use std::os::unix::io::AsRawFd;

        if timeout > 0 {
            setsockopt(self.as_raw_fd(),
                       sockopt::ReceiveTimeout,
                       &TimeVal::milliseconds(timeout)).unwrap();
        }

        fn map_os_error(e: Error) -> Error {
            // TODO: Replace with constant from libc
            const EAGAIN: i32 = 35;

            match e.raw_os_error() {
                Some(EAGAIN) => Error::new(ErrorKind::WouldBlock, ""),
                _ => e
            }
        }

        self.recv_from(buf).map_err(map_os_error)
    }

    #[cfg(windows)]
    fn recv_timeout(&mut self, buf: &mut [u8], timeout: i64) -> Result<(usize, SocketAddr)> {
        use select;
        use select::fd_set;
        use std::os::windows::io::AsRawSocket;
        use std::ptr;
        use libc;

        if timeout > 0 {
            // Initialize relevant data structures
            let mut readfds = fd_set::new();
            let null = ptr::null_mut();

            fd_set(&mut readfds, self.as_raw_socket());

            // Set timeout
            let mut tv = libc::timeval {
                tv_sec: timeout as i32 / 1000,
                tv_usec: (timeout as i32 % 1000) * 1000,
            };

            // In Windows, the first argument to `select` is ignored.
            let retval = unsafe { select::select(0, &mut readfds, null, null, &mut tv) };
            if retval == 0 {
                return Err(Error::new(ErrorKind::TimedOut, "Time limit expired"));
            } else if retval < 0 {
                return Err(Error::last_os_error());
            }
        }

        self.recv_from(buf)
    }
}

#[test]
fn test_socket_timeout() {
    let mut socket = UdpSocket::bind("0.0.0.0:0").unwrap();
    let mut buf = [0; 10];
    assert!(socket.recv_timeout(&mut buf, 100).is_err());
}
