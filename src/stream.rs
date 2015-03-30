use std::io::{Read, Write, Result};
use std::net::{ToSocketAddrs};
use socket::UtpSocket;

/// Stream interface for UtpSocket.
pub struct UtpStream {
    socket: UtpSocket,
}

impl UtpStream {
    /// Create a uTP stream listening on the given address.
    #[unstable]
    pub fn bind<A: ToSocketAddrs>(addr: A) -> Result<UtpStream> {
        match UtpSocket::bind(addr) {
            Ok(s)  => Ok(UtpStream { socket: s }),
            Err(e) => Err(e),
        }
    }

    /// Open a uTP connection to a remote host by hostname or IP address.
    #[unstable]
    pub fn connect<A: ToSocketAddrs>(dst: A) -> Result<UtpStream> {
        // Port 0 means the operating system gets to choose it
        let my_addr = "0.0.0.0:0";
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
    pub fn close(&mut self) -> Result<()> {
        self.socket.close()
    }
}

impl Read for UtpStream {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        match self.socket.recv_from(buf) {
            Ok((read, _src)) => Ok(read),
            Err(e) => Err(e),
        }
    }
}

impl Write for UtpStream {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        self.socket.send_to(buf)
    }

    // TODO: Actually implement flushing
    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}
