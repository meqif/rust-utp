use std::io::{Read, Write, Result};
use std::net::{ToSocketAddrs};
use socket::UtpSocket;

/// Stream interface for UtpSocket.
pub struct UtpStream {
    socket: UtpSocket,
}

impl UtpStream {
    /// Create a uTP stream listening on the given address.
    pub fn bind<A: ToSocketAddrs>(addr: A) -> Result<UtpStream> {
        UtpSocket::bind(addr).and_then(|s| Ok(UtpStream { socket: s }))
    }

    /// Open a uTP connection to a remote host by hostname or IP address.
    pub fn connect<A: ToSocketAddrs>(dst: A) -> Result<UtpStream> {
        // Port 0 means the operating system gets to choose it
        let my_addr = "0.0.0.0:0";
        UtpSocket::bind(my_addr)
            .and_then(|s| s.connect(dst))
            .and_then(|s| Ok(UtpStream { socket: s }))
    }

    /// Gracefully close connection to peer.
    ///
    /// This method allows both peers to receive all packets still in
    /// flight.
    pub fn close(&mut self) -> Result<()> {
        self.socket.close()
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

    // TODO: Actually implement flushing
    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}
