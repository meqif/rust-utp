#[cfg(unix)]
extern crate nix;
#[cfg(windows)]
extern crate libc;

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
        use nix::sys::socket::{SockLevel, sockopt, setsockopt};
        use nix::sys::time::TimeVal;
        use std::os::unix::io::AsRawFd;

        setsockopt(self.as_raw_fd(),
                   SockLevel::Socket,
                   sockopt::ReceiveTimeout,
                   &TimeVal::milliseconds(timeout)).unwrap();

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
        use select::fd_set;
        use std::os::windows::io::AsRawSocket;
        use libc;

        // Initialize relevant data structures
        let mut readfds = fd_set::new();
        let null = std::ptr::null_mut();

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

        self.recv_from(buf)
    }
}

// Most of the following was copied from 'rust/src/libstd/sys/windows/net.rs'
#[cfg(windows)]
mod select {
    use libc;

    pub const FD_SETSIZE: usize = 64;

    #[repr(C)]
    pub struct fd_set {
        fd_count: libc::c_uint,
        fd_array: [libc::SOCKET; FD_SETSIZE],
    }

    pub fn fd_set(set: &mut fd_set, s: libc::SOCKET) {
        set.fd_array[set.fd_count as usize] = s;
        set.fd_count += 1;
    }

    impl fd_set {
        pub fn new() -> fd_set {
            fd_set {
                fd_count: 0,
                fd_array: [0; FD_SETSIZE],
            }
        }
    }

    #[link(name = "ws2_32")]
    extern "system" {
        pub fn select(nfds: libc::c_int,
                      readfds: *mut fd_set,
                      writefds: *mut fd_set,
                      exceptfds: *mut fd_set,
                      timeout: *mut libc::timeval) -> libc::c_int;
    }
}

#[test]
fn test_socket_timeout() {
    let mut socket = UdpSocket::bind("0.0.0.0:0").unwrap();
    let mut buf = [0; 10];
    assert!(socket.recv_timeout(&mut buf, 100).is_err());
}
