use std::io::IoResult;
use std::io::net::ip::SocketAddr;
use socket::UtpSocket;

/// Stream interface for UtpSocket.
pub struct UtpStream {
    socket: UtpSocket,
}

impl UtpStream {
    /// Create a uTP stream listening on the given address.
    #[unstable]
    pub fn bind(addr: SocketAddr) -> IoResult<UtpStream> {
        match UtpSocket::bind(addr) {
            Ok(s)  => Ok(UtpStream { socket: s }),
            Err(e) => Err(e),
        }
    }

    /// Open a uTP connection to a remote host by hostname or IP address.
    #[unstable]
    pub fn connect(dst: SocketAddr) -> IoResult<UtpStream> {
        use std::io::net::ip::Ipv4Addr;

        // Port 0 means the operating system gets to choose it
        let my_addr = SocketAddr { ip: Ipv4Addr(0,0,0,0), port: 0 };
        let socket = match UtpSocket::bind(my_addr) {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

        match socket.connect(dst) {
            Ok(socket) => Ok(UtpStream { socket: socket }),
            Err(e) => Err(e),
        }
    }

    /// Gracefully close connection to peer.
    ///
    /// This method allows both peers to receive all packets still in
    /// flight.
    #[unstable]
    pub fn close(&mut self) -> IoResult<()> {
        self.socket.close()
    }
}

impl Reader for UtpStream {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<uint> {
        match self.socket.recv_from(buf) {
            Ok((read, _src)) => Ok(read),
            Err(e) => Err(e),
        }
    }
}

impl Writer for UtpStream {
    fn write(&mut self, buf: &[u8]) -> IoResult<()> {
        self.socket.send_to(buf)
    }
}
