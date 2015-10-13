// Most of the following was copied from 'rust/src/libstd/sys/windows/net.rs'
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
