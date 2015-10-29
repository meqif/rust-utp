use std::io::{Read, Write, Result};
use std::net::{ToSocketAddrs, SocketAddr, UdpSocket};
use std::ops::Deref;
use socket::UtpSocket;

/// A structure that represents a uTP (Micro Transport Protocol) stream between a local socket and a
/// remote socket.
///
/// The connection will be closed when the value is dropped (either explicitly or when it goes out
/// of scope).
///
/// The default maximum retransmission retries is 5, which translates to about 16 seconds. It can be
/// changed by calling `set_max_retransmission_retries`. Notice that the initial congestion timeout
/// is 500 ms and doubles with each timeout.
///
/// # Examples
///
/// ```no_run
/// use utp::UtpStream;
/// use std::io::{Read, Write};
///
/// let mut stream = UtpStream::bind("127.0.0.1:1234").unwrap();
/// let _ = stream.write(&[1]);
/// let _ = stream.read(&mut [0; 1000]);
/// ```
pub struct UtpStream {
    socket: UtpSocket,
}

impl UtpStream {
    /// Creates a uTP stream listening on the given address.
    ///
    /// The address type can be any implementer of the `ToSocketAddr` trait. See its documentation
    /// for concrete examples.
    ///
    /// If more than one valid address is specified, only the first will be used.
    pub fn bind<A: ToSocketAddrs>(addr: A) -> Result<UtpStream> {
        UtpSocket::bind(addr).map(|s| UtpStream { socket: s })
    }

    /// Creates a uTP stream reusing the given UDP socket.
    pub fn bind_with_udp_socket(udp_socket: UdpSocket) -> Result<UtpStream> {
        UtpSocket::bind_with_udp_socket(udp_socket).map(|s| UtpStream { socket : s})
    }

    /// Opens a uTP connection to a remote host by hostname or IP address.
    ///
    /// The address type can be any implementer of the `ToSocketAddr` trait. See its documentation
    /// for concrete examples.
    ///
    /// If more than one valid address is specified, only the first will be used.
    pub fn connect<A: ToSocketAddrs>(dst: A) -> Result<UtpStream> {
        // Port 0 means the operating system gets to choose it
        UtpSocket::connect(dst).map(|s| UtpStream { socket: s })
    }

    /// Opens a uTP connection to a remote host, reusing the given UDP socket.
    ///
    /// The address type can be any implementer of the `ToSocketAddr` trait. See its documentation
    /// for concrete examples.
    ///
    /// If more than one valid address is specified, only the first will be used.
    pub fn connect_with_udp_socket<A: ToSocketAddrs>(udp_socket: UdpSocket, dst: A)
        -> Result<UtpStream>
    {
        UtpSocket::connect_with_udp_socket(udp_socket, dst).map(|s| UtpStream { socket: s})
    }

    /// Gracefully closes connection to peer.
    ///
    /// This method allows both peers to receive all packets still in
    /// flight.
    pub fn close(&mut self) -> Result<()> {
        self.socket.close()
    }

    /// Returns the socket address of the local half of this uTP connection.
    pub fn local_addr(&self) -> Result<SocketAddr> {
        self.socket.local_addr()
    }

    /// Changes the maximum number of retransmission retries on the underlying socket.
    pub fn set_max_retransmission_retries(&mut self, n: u32) {
        self.socket.max_retransmission_retries = n;
    }
}

impl Read for UtpStream {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        self.socket.recv_from(buf).map(|(read, _src)| read)
    }
}

impl Write for UtpStream {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        self.socket.send_to(buf)
    }

    fn flush(&mut self) -> Result<()> {
        self.socket.flush()
    }
}

impl Into<UtpStream> for UtpSocket {
    fn into(self) -> UtpStream {
        UtpStream { socket: self }
    }
}

impl Deref for UtpStream {
    type Target = UtpSocket;

    fn deref(&self) -> &UtpSocket {
        &self.socket
    }
}
