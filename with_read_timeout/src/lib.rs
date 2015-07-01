#![feature(libc)]

#[cfg(unix)]
extern crate nix;
extern crate libc;

use std::io::Result;
use std::net::{UdpSocket, SocketAddr};
pub use self::select::RecvTimeoutCtx;

/// A trait to make time-limited reads from socket-like objects.
pub trait WithReadTimeout {
    /// Returns an object with which a recv_timeout can be broken
    fn recv_timeout_init() -> RecvTimeoutCtx;
    
    /// Receives data from the object, blocking for at most the specified number of milliseconds.
    /// On success, returns the number of bytes read and the address from whence the data came.  If
    /// the timeout expires, it returns `ErrorKind::TimedOut`.
    fn recv_timeout(&mut self, ctx : &RecvTimeoutCtx, &mut [u8], i64) -> Result<(usize, SocketAddr)>;
}

impl WithReadTimeout for UdpSocket {
    fn recv_timeout_init() -> RecvTimeoutCtx {
        RecvTimeoutCtx::new()
    }

    fn recv_timeout(&mut self, ctx : &RecvTimeoutCtx, buf: &mut [u8], timeout: i64) -> Result<(usize, SocketAddr)> {
        select::recv_timeout(self, ctx, buf, timeout)
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
    use std::net::{UdpSocket, SocketAddr};
    use std::os::unix::io::AsRawFd;
    use libc;
    use std::sync::Mutex;
    use std::io::{Error, ErrorKind, Result};

    const SIGUSR1: libc::c_int = 10;
    const SIG_BLOCK: libc::c_int = 1;
    pub const FD_SETSIZE: usize = 1024;
    pub const ULONG_BITS: usize = 8*8;  // FIXME: How do I actually calculate this? size_of isn't constexpr :(
    #[allow(non_camel_case_types)]
    pub type pthread_t = libc::size_t;

    #[repr(C)]
    pub struct fd_set {
        pub fds_bits: [libc::c_ulong; (FD_SETSIZE / ULONG_BITS)]
    }
    
    impl fd_set {
        pub fn new() -> fd_set {
            fd_set {
                fds_bits: [0; (FD_SETSIZE / ULONG_BITS)],
            }
        }
    }
    
    #[repr(C)]
    pub struct sigset_t {
        pub sig: [libc::c_ulong; 2]
    }

    impl sigset_t {
        pub fn new() -> sigset_t {
            sigset_t {
                sig: [0; 2],
            }
        }
    }
    
    pub fn nfds(socket : &UdpSocket) -> libc::c_int {
        socket.as_raw_fd() + 1
    }

    pub fn fd_set(set: &mut fd_set, socket : &UdpSocket) {
        let fd = socket.as_raw_fd() as usize;
        set.fds_bits[fd / ULONG_BITS] |= 1 << (fd % ULONG_BITS);
    }

    //pub fn fd_zero(set: &mut fd_set) {
    //    set.fds_bits = [0; (FD_SETSIZE / ULONG_BITS)];
    //}

    pub struct RecvTimeoutCtx {
        pub waiters: Mutex<(bool, Vec<pthread_t>)>,
    }
    
    extern fn handle_signal(_:i32) {
        // Do nothing, let EINTR take care of things
    }

    impl RecvTimeoutCtx {
        pub fn new() -> RecvTimeoutCtx {
            RecvTimeoutCtx { waiters: Mutex::<(bool, Vec<pthread_t>)>::new((false, Vec::<pthread_t>::new())) }
        }
        
        pub fn add_waiter(&self) -> Result<bool> {
            use std::ptr;
            use nix::sys::signal;
            let mut data = self.waiters.lock().unwrap();
            if (*data).0 {
                return Ok(false);
            }
            // Disable SIGUSR1 on this thread and install a null signal handler for it
            let null = ptr::null_mut();
            let mut sigset = sigset_t::new();
            sigset.sig[0]=1<<SIGUSR1;
            let _ = unsafe { pthread_sigmask(SIG_BLOCK, &sigset, null) };
            let sig_action = signal::SigAction::new(handle_signal, signal::SockFlag::empty(), signal::SigSet::empty());
            let _ = unsafe { signal::sigaction(SIGUSR1, &sig_action) };
            
            (*data).1.push(unsafe { pthread_self() });
            //println!("Thread {} enters wait", unsafe { pthread_self() });
            Ok(true)
        }

        pub fn remove_waiter(&self) {
            let mut data = self.waiters.lock().unwrap();
            // Is it really this hard to remove a value from a vector in Rust?
            let toremove = unsafe { pthread_self() };
            (*data).1.iter().position(|&x| x==toremove).map(|x| (*data).1.remove(x));
            //println!("Thread {} exits wait. Waiters remaining:", toremove);
            //for waiter in &(*data).1 {
            //    println!("   thread {}", *waiter);
            //}
        }

        pub fn break_reads(&self) -> Result<()> {
            let mut done = false;
            while !done {
                let mut data = self.waiters.lock().unwrap();
                (*data).0=true;
                done = (*data).1.is_empty();
                for waiter in &(*data).1 {
                    //println!("We break wait for thread {}", *waiter);
                    if -1 == unsafe { pthread_kill(*waiter, SIGUSR1) } {
                        return Err(Error::last_os_error())
                    }
                }
            }
            Ok(())
        }
    }

    impl Drop for RecvTimeoutCtx {
        fn drop(&mut self) {
            let _ = self.break_reads();
        }
    }

    pub fn recv_timeout(socket: &mut UdpSocket, ctx : &RecvTimeoutCtx, buf: &mut [u8], timeout: i64) -> Result<(usize, SocketAddr)> {
        use nix::sys::socket::{SockLevel, sockopt, setsockopt};
        use nix::sys::time::TimeVal;
        use std::os::unix::io::AsRawFd;
        use std::ptr;

        // Initialize relevant data structures
        let mut readfds = fd_set::new();
        let null = ptr::null_mut();

        fd_set(&mut readfds, &socket);
        let nfds = nfds(&socket);

        // Set timeout
        let mut ts = libc::timespec {
            tv_sec: timeout / 1000,
            tv_nsec: (timeout % 1000) * 1000000,
        };
        
        let mut sigset = sigset_t::new();
        sigset.sig[0]=1<<SIGUSR1;

        if try!(ctx.add_waiter()) {
            let retval = unsafe { pselect(nfds, &mut readfds, null, null, &mut ts, &sigset) };
            ctx.remove_waiter();
            if retval == 0 {
                return Err(Error::new(ErrorKind::TimedOut, "Time limit expired"));
            } else if retval < 0 {
                return Err(Error::last_os_error());
            }
        }
        
        // select() is of course racy to blocking reads, so temporarily set this socket to non-blocking before we read
        setsockopt(socket.as_raw_fd(),
                   SockLevel::Socket,
                   sockopt::ReceiveTimeout,
                   &TimeVal::microseconds(1)).unwrap();

        fn map_os_error(e: Error) -> Error {
            // TODO: Replace with constant from libc
            const EAGAIN: i32 = 35;

            match e.raw_os_error() {
                Some(EAGAIN) => Error::new(ErrorKind::WouldBlock, ""),
                _ => e
            }
        }
            
        let ret = socket.recv_from(buf).map_err(map_os_error);

        setsockopt(socket.as_raw_fd(),
                   SockLevel::Socket,
                   sockopt::ReceiveTimeout,
                   &TimeVal::microseconds(0)).unwrap();
                   
        ret
    }

    #[link(name = "pthread")]
    extern {
        pub fn pselect(nfds: libc::c_int,
                  readfds: *mut fd_set,
                  writefds: *mut fd_set,
                  errorfds: *mut fd_set,
                  timeout: *mut libc::timespec,
                  sigmask: *const sigset_t) -> libc::c_int;
        pub fn pthread_self() -> pthread_t;
        pub fn pthread_kill(id: pthread_t, signal: libc::c_int) -> libc::c_int;
        pub fn pthread_sigmask(how: libc::c_int, set: *const sigset_t, oldset: *mut sigset_t) -> libc::c_int;
    }
}


// Most of the following was copied from 'rust/src/libstd/sys/windows/net.rs'
#[cfg(windows)]
mod select {
    use std::net::{UdpSocket, SocketAddr};
    use std::os::windows::io::AsRawSocket;
    use std::ptr;
    use libc;
    use std::io::{Error, ErrorKind, Result};

    pub struct RecvTimeoutCtx {
        pub eventh : libc::HANDLE,
    }
    
    impl RecvTimeoutCtx {
        pub fn new() -> RecvTimeoutCtx {
            RecvTimeoutCtx { eventh: unsafe { CreateEventW(ptr::null_mut(), 1, 0, ptr::null_mut()) } }
        }
        
        pub fn break_reads(&self) -> Result<()> {
            if !unsafe { SetEvent(self.eventh) } {
                Err(Error::last_os_error())
            }
            Ok(())
        }
    }

    impl Drop for RecvTimeoutCtx {
        fn drop(&mut self) {
            let _ = self.break_reads();
            let _ = unsafe { CloseHandle(self.eventh) };
        }
    }

    pub fn recv_timeout(socket: &mut UdpSocket, ctx : &RecvTimeoutCtx, buf: &mut [u8], timeout: i64) -> Result<(usize, SocketAddr)> {

        let mut handles : [libc::HANDLE; 2];
        handles[0] = ctx.eventh;
        handles[1] = socket.as_raw_socket();
        let retval = unsafe { WaitForMultipleObjects(2, &handles, 0, timeout as libc::DWORD) };
        if retval == 0xffffffff as libc::DWORD {
            return Err(Error::last_os_error());
        } else if retval == 0x102 /* WAIT_TIMEOUT */ {
            return Err(Error::new(ErrorKind::TimedOut, "Time limit expired"));
        } else if retval == 0 {
            return Err(Error::new(ErrorKind::WouldBlock, ""));
        }
        
        socket.recv_from(buf)
    }

    #[link(name = "kernel32")]
    extern "system" {
        pub fn CreateEventW(lpEventAttributes: libc::c_void,
                            bManualReset: libc::BOOL,
                            bInitialState: libc::BOOL,
                            lpName: libc::c_void) -> libc::HANDLE;
        pub fn CloseHandle(hEvent: libc::HANDLE) -> libc::BOOL;
        pub fn SetEvent(hEvent: libc::HANDLE) -> libc::BOOL;
        pub fn WaitForMultipleObjects(nCount: libc::DWORD,
                                      lpHandles: *const libc::HANDLE,
                                      bWaitAll: libc::BOOL,
                                      dwMilliseconds: libc::DWORD) -> libc::DWORD;
    }
}

#[test]
fn test_socket_timeout() {
    let mut socket = UdpSocket::bind("0.0.0.0:0").unwrap();
    let mut buf = [0; 10];
    assert!(socket.recv_timeout(&mut buf, 100).is_err());
}
