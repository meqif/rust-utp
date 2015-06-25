extern crate libc;

use std::io::{Error, ErrorKind, Result};
use std::net::{UdpSocket, SocketAddr};

/// A trait to make time-limited reads from socket-like objects.
pub trait WithReadTimeout {
    /// Returns an object with which a recv_timeout can be broken
    fn recv_timeout_init() -> Result<RecvTimeoutCtx>;
    
    /// Receives data from the object, blocking for at most the specified number of milliseconds.
    /// On success, returns the number of bytes read and the address from whence the data came.  If
    /// the timeout expires, it returns `ErrorKind::TimedOut`.
    fn recv_timeout(&mut self, ctx : &RecvTimeoutCtx, &mut [u8], i64) -> Result<(usize, SocketAddr)>;
}

/// RAII handle for breaking recv_timeout's
pub struct RecvTimeoutCtx {
    pub pipefd : [libc::c_int; 2],
}

impl RecvTimeoutCtx {
    pub fn new() -> Result<RecvTimeoutCtx> {
        let r = RecvTimeoutCtx { pipefd : [0; 2] };
        if -1 == unsafe { select::pipe(&r.pipefd) } {
            Err(Error::last_os_error())
        } else {
            Ok(r)
        }
    }

    pub fn break_reads(&self) -> Result<()> {
        let b : u8 = 0;
        if -1 == unsafe { select::write(self.pipefd[1], &b, 1) } {
            Err(Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

impl Drop for RecvTimeoutCtx {
    fn drop(&mut self) {
        unsafe {
            select::close(self.pipefd[0]);
            select::close(self.pipefd[1]);
        }
    }
}

impl WithReadTimeout for UdpSocket {
    fn recv_timeout_init() -> Result<RecvTimeoutCtx> {
        RecvTimeoutCtx::new()
    }
    
    fn recv_timeout(&mut self, ctx : &RecvTimeoutCtx, buf: &mut [u8], timeout: i64) -> Result<(usize, SocketAddr)> {
        use select::{fd_set, fd_zero, nfds};
        use libc;

        // Initialize relevant data structures
        let mut readfds = fd_set::new(ctx);
        let null = std::ptr::null_mut();

        fd_set(&mut readfds, &self);
        let nfds = select::nfds(&self, ctx);

        // Set timeout
        let mut tv = libc::timeval {
            tv_sec: timeout / 1000,
            tv_usec: (timeout % 1000) * 1000,
        };

        let retval = unsafe { select::select(nfds, &mut readfds, null, null, &mut tv) };
        if retval == 0 {
            return Err(Error::new(ErrorKind::TimedOut, "Time limit expired"));
        } else if retval < 0 {
            return Err(Error::last_os_error());
        }
        
        // Was it the breaking fd?
        if select::break_set(&readfds, ctx) {
            // If socket fd also set, might as well read
            // Note that on Linux multi select() can claim the fd is ready for reading, but still block
            // so run select() again on just the UDP socket alone
            if retval == 2 {
                fd_zero(&mut readfds);
                fd_set(&mut readfds, &self);
                let mut tv = libc::timeval {
                    tv_sec: 0,
                    tv_usec: 0,
                };
                let retval = unsafe { select::select(nfds, &mut readfds, null, null, &mut tv) };
                if retval == 0 {
                    return Err(Error::new(ErrorKind::TimedOut, "Time limit expired"));
                } else {
                    return self.recv_from(buf)
                }
            } else {
                return Err(Error::new(ErrorKind::TimedOut, "Time limit expired"));
            }
        }

        self.recv_from(buf)
    }
}

// Most of the following was copied from 'rust/src/libstd/sys/unix/c.rs'
#[cfg(any(target_os = "macos", target_os = "ios"))]
mod select {
    use std::cmp;
    use std::net::UdpSocket;
    use std::os::unix::io::AsRawFd;
    use super::RecvTimeoutCtx;
    pub const FD_SETSIZE: usize = 1024;
    
    #[repr(C)]
    pub struct fd_set {
        fds_bits: [i32; (FD_SETSIZE / 32)]
    }
    
    pub fn nfds(socket : &UdpSocket, ctx : &RecvTimeoutCtx) -> libc::c_int {
        cmp::max(socket.as_raw_fd(), ctx.pipefd[0]) + 1
    }

    pub fn break_set(set: &fd_set, ctx : &RecvTimeoutCtx) -> bool {
        (set.fds_bits[(ctx.pipefd[0] / 32) as usize] & (1 << ((ctx.pipefd[0] % 32) as usize))) != 0
    }

    pub fn fd_set(set: &mut fd_set, socket : &UdpSocket) {
        let fd = socket.as_raw_fd() as usize;
        set.fds_bits[(fd / 32) as usize] |= 1 << ((fd % 32) as usize);
    }
    
    pub fn fd_zero(set: &mut fd_set) {
        set.fds_bits = [0; (FD_SETSIZE / 32)];
    }

    impl fd_set {
        pub fn new(ctx : &RecvTimeoutCtx) -> fd_set {
            let mut set = fd_set {
                fds_bits: [0; (FD_SETSIZE / 32)],
            };
            set.fds_bits[(ctx.pipefd[0] / 32) as usize] |= 1 << ((ctx.pipefd[0] % 32) as usize);
            set
        }
    }
    
    extern {
        pub fn close(fd: libc::c_int) -> libc::c_int;
        pub fn pipe(pipefd: &[libc::c_int; 2]) -> libc::c_int;
        pub fn select(nfds: libc::c_int,
                  readfds: *mut fd_set,
                  writefds: *mut fd_set,
                  errorfds: *mut fd_set,
                  timeout: *mut libc::timeval) -> libc::c_int;
        pub fn write(fd: libc::c_int, buf : *const u8, count : libc::size_t) -> libc::c_int;
    }
}

#[cfg(any(target_os = "android",
          target_os = "freebsd",
          target_os = "dragonfly",
          target_os = "bitrig",
          target_os = "openbsd",
          target_os = "linux"))]
mod select {
    use std::cmp;
    use std::net::UdpSocket;
    use super::RecvTimeoutCtx;
    use std::os::unix::io::AsRawFd;
    use libc;

    pub const FD_SETSIZE: usize = 1024;
    pub const ULONG_BITS: usize = 8*8;  // FIXME: How do I actually calculate this? size_of isn't constexpr :(

    #[repr(C)]
    pub struct fd_set {
        fds_bits: [libc::c_ulong; (FD_SETSIZE / ULONG_BITS)]
    }

    pub fn nfds(socket : &UdpSocket, ctx : &RecvTimeoutCtx) -> libc::c_int {
        cmp::max(socket.as_raw_fd(), ctx.pipefd[0]) + 1
    }

    pub fn break_set(set: &fd_set, ctx : &RecvTimeoutCtx) -> bool {
        (set.fds_bits[ctx.pipefd[0] as usize / ULONG_BITS] & (1 << (ctx.pipefd[0] as usize % ULONG_BITS))) != 0
    }

    pub fn fd_set(set: &mut fd_set, socket : &UdpSocket) {
        let fd = socket.as_raw_fd() as usize;
        set.fds_bits[fd / ULONG_BITS] |= 1 << (fd % ULONG_BITS);
    }

    pub fn fd_zero(set: &mut fd_set) {
        set.fds_bits = [0; (FD_SETSIZE / ULONG_BITS)];
    }

    impl fd_set {
        pub fn new(ctx : &RecvTimeoutCtx) -> fd_set {
            let mut set = fd_set {
                fds_bits: [0; (FD_SETSIZE / ULONG_BITS)],
            };
            set.fds_bits[ctx.pipefd[0] as usize / ULONG_BITS] |= 1 << (ctx.pipefd[0] as usize % ULONG_BITS);
            set
        }
    }
    
    extern {
        pub fn close(fd: libc::c_int) -> libc::c_int;
        pub fn pipe(pipefd: &[libc::c_int; 2]) -> libc::c_int;
        pub fn select(nfds: libc::c_int,
                  readfds: *mut fd_set,
                  writefds: *mut fd_set,
                  errorfds: *mut fd_set,
                  timeout: *mut libc::timeval) -> libc::c_int;
        pub fn write(fd: libc::c_int, buf : *const u8, count : libc::size_t) -> libc::c_int;
    }
}


// Most of the following was copied from 'rust/src/libstd/sys/windows/net.rs'
#[cfg(windows)]
mod select {
    use std::net::UdpSocket;
    use std::os::windows::io::AsRawSocket;
    use libc;

    pub const FD_SETSIZE: usize = 64;

    #[repr(C)]
    pub struct fd_set {
        fd_count: libc::c_uint,
        fd_array: [libc::SOCKET; FD_SETSIZE],
    }

    pub fn nfds(socket : &UdpSocket) -> libc::c_int {
        0
    }

    pub fn fd_set(set: &mut fd_set, socket : &UdpSocket) {
        let s = socket.as_raw_handle();
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
