//! Implementation of a Micro Transport Protocol library.
//!
//! http://www.bittorrent.org/beps/bep_0029.html
//!
//! TODO
//! ----
//!
//! - congestion control
//! - proper connection closing
//!     - handle both RST and FIN
//!     - send FIN on close
//!     - automatically send FIN (or should it be RST?) on drop if not already closed
//! - sending RST on mismatch
//! - setters and getters that hide header field endianness conversion
//! - SACK extension
//! - packet loss
//! - test UtpSocket

#![feature(macro_rules)]

extern crate time;

use std::io::net::udp::UdpSocket;
use std::io::net::ip::SocketAddr;
use std::io::IoResult;
use std::mem::transmute;
use std::rand::random;
use std::fmt;

static HEADER_SIZE: uint = 20;
static BUF_SIZE: uint = 4096;

/// Return current time in microseconds since the UNIX epoch.
fn now_microseconds() -> u32 {
    let t = time::get_time();
    (t.sec * 1_000_000) as u32 + (t.nsec/1000) as u32
}

#[allow(dead_code,non_camel_case_types)]
#[deriving(PartialEq,Eq,Show)]
enum UtpPacketType {
    ST_DATA  = 0,
    ST_FIN   = 1,
    ST_STATE = 2,
    ST_RESET = 3,
    ST_SYN   = 4,
}

#[allow(dead_code)]
#[packed]
struct UtpPacketHeader {
    type_ver: u8, // type: u4, ver: u4
    extension: u8,
    connection_id: u16,
    timestamp_microseconds: u32,
    timestamp_difference_microseconds: u32,
    wnd_size: u32,
    seq_nr: u16,
    ack_nr: u16,
}

impl UtpPacketHeader {
    /// Set type of packet to the specified type.
    fn set_type(&mut self, t: UtpPacketType) {
        let version = 0x0F & self.type_ver;
        self.type_ver = t as u8 << 4 | version;
    }

    fn get_type(&self) -> UtpPacketType {
        let t: UtpPacketType = unsafe { transmute(self.type_ver >> 4) };
        t
    }

    fn get_version(&self) -> u8 {
        self.type_ver & 0x0F
    }

    /// Return packet header as a slice of bytes.
    fn bytes(&self) -> &[u8] {
        let buf: &[u8, ..HEADER_SIZE] = unsafe { transmute(self) };
        return buf.as_slice();
    }

    fn len(&self) -> uint {
        return HEADER_SIZE;
    }

    /// Read byte buffer and return corresponding packet header.
    /// It assumes the fields are in network (big-endian) byte order,
    /// preserving it.
    fn decode(buf: &[u8]) -> UtpPacketHeader {
        UtpPacketHeader {
            type_ver: buf[0],
            extension: buf[1],
            connection_id: buf[3] as u16 << 8 | buf[2] as u16,
            timestamp_microseconds: buf[7] as u32 << 24 | buf[6] as u32 << 16 | buf[5] as u32 << 8 | buf[4] as u32,
            timestamp_difference_microseconds: buf[11] as u32 << 24 | buf[10] as u32 << 16 | buf[9] as u32 << 8 | buf[8] as u32,
            wnd_size: buf[15] as u32 << 24 | buf[14] as u32 << 16 | buf[13] as u32 << 8 | buf[12] as u32,
            seq_nr: buf[17] as u16 << 8 | buf[16] as u16,
            ack_nr: buf[19] as u16 << 8 | buf[18] as u16,
        }
    }
}

impl fmt::Show for UtpPacketHeader {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "(type: {}, version: {}, extension: {}, \
                connection_id: {}, timestamp_microseconds: {}, \
                timestamp_difference_microseconds: {}, wnd_size: {}, \
                seq_nr: {}, ack_nr: {})",
                self.get_type(),
                Int::from_be(self.get_version()),
                Int::from_be(self.extension),
                Int::from_be(self.connection_id),
                Int::from_be(self.timestamp_microseconds),
                Int::from_be(self.timestamp_difference_microseconds),
                Int::from_be(self.wnd_size),
                Int::from_be(self.seq_nr),
                Int::from_be(self.ack_nr),
        )
    }
}

#[allow(dead_code)]
struct UtpPacket {
    header: UtpPacketHeader,
    payload: Vec<u8>,
}

impl UtpPacket {
    /// Construct a new, empty packet.
    fn new() -> UtpPacket {
        UtpPacket {
            header: UtpPacketHeader {
                type_ver: ST_DATA as u8 << 4 | 1,
                extension: 0,
                connection_id: 0,
                timestamp_microseconds: 0,
                timestamp_difference_microseconds: 0,
                wnd_size: 0,
                seq_nr: 0,
                ack_nr: 0,
            },
            payload: Vec::new(),
        }
    }

    fn set_type(&mut self, t: UtpPacketType) {
        self.header.set_type(t);
    }

    // TODO: Read up on pointers and ownership
    fn get_type(&self) -> UtpPacketType {
        self.header.get_type()
    }

    fn wnd_size(&self, new_wnd_size: u32) -> UtpPacket {
        UtpPacket {
            header: UtpPacketHeader {
                type_ver: self.header.type_ver,
                extension: self.header.extension,
                connection_id: self.header.connection_id,
                timestamp_microseconds: self.header.timestamp_microseconds,
                timestamp_difference_microseconds: self.header.timestamp_difference_microseconds,
                wnd_size: new_wnd_size.to_be(),
                seq_nr: self.header.seq_nr,
                ack_nr: self.header.ack_nr,
            },
            payload: self.payload.clone(),
        }
    }

    /// TODO: return slice
    fn bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(self.len());
        buf.push_all(self.header.bytes());
        buf.push_all(self.payload.as_slice());
        return buf;
    }

    fn len(&self) -> uint {
        self.header.len() + self.payload.len()
    }

    fn decode(buf: &[u8]) -> UtpPacket {
        UtpPacket {
            header:  UtpPacketHeader::decode(buf),
            payload: Vec::from_slice(buf.slice(HEADER_SIZE, buf.len()))
        }
    }
}

impl Clone for UtpPacket {
    fn clone(&self) -> UtpPacket {
        UtpPacket {
            header:  self.header,
            payload: self.payload.clone(),
        }
    }
}

#[allow(non_camel_case_types)]
#[deriving(PartialEq,Eq,Show)]
enum UtpSocketState {
    CS_NEW,
    CS_CONNECTED,
    CS_SYN_SENT,
    CS_FIN_RECEIVED,
    CS_RST_RECEIVED,
    CS_CLOSED,
}

/// A uTP (Micro Transport Protocol) socket.
pub struct UtpSocket {
    socket: UdpSocket,
    connected_to: SocketAddr,
    sender_connection_id: u16,
    receiver_connection_id: u16,
    seq_nr: u16,
    ack_nr: u16,
    state: UtpSocketState,
}

macro_rules! reply_with_ack(
    ($header:expr, $src:expr) => ({
        let resp = self.prepare_reply($header, ST_STATE).wnd_size(BUF_SIZE as u32);
        try!(self.socket.sendto(resp.bytes().as_slice(), $src));
        if cfg!(not(test)) { println!("sent {}", resp.header) };
    })
)

impl UtpSocket {
    pub fn bind(addr: SocketAddr) -> IoResult<UtpSocket> {
        let skt = UdpSocket::bind(addr);
        let r: u16 = random();
        match skt {
            Ok(x)  => Ok(UtpSocket {
                socket: x,
                connected_to: addr,
                receiver_connection_id: r,
                sender_connection_id: r + 1,
                seq_nr: 1,
                ack_nr: 0,
                state: CS_NEW,
            }),
            Err(e) => Err(e)
        }
    }

    /// Open a uTP connection to a remote host by hostname or IP address.
    pub fn connect(mut self, other: SocketAddr) -> UtpSocket {
        self.connected_to = other;
        assert_eq!(self.receiver_connection_id + 1, self.sender_connection_id);

        let mut packet = UtpPacket::new();
        packet.set_type(ST_SYN);
        packet.header.connection_id = self.receiver_connection_id.to_be();
        packet.header.seq_nr = self.seq_nr.to_be();
        packet.header.timestamp_microseconds = now_microseconds().to_be();

        // Send packet
        let dst = self.connected_to;
        let _result = self.socket.sendto(packet.bytes().as_slice(), dst);
        if cfg!(not(test)) { println!("sent {}", packet.header) };

        self.state = CS_SYN_SENT;

        let mut buf = [0, ..BUF_SIZE];
        let (_len, addr) = self.recvfrom(buf).unwrap();
        if cfg!(not(test)) { println!("connected to: {} {}", addr, self.connected_to) };
        assert!(addr == self.connected_to);

        self.state = CS_CONNECTED;
        self.seq_nr += 1;

        self
    }

    /// Gracefully close connection to peer.
    ///
    /// This method allows both peers to receive all packets still in
    /// flight.
    pub fn close(&mut self) -> IoResult<()> {
        let mut packet = UtpPacket::new();
        packet.header.connection_id = self.sender_connection_id.to_be();
        packet.header.seq_nr = self.seq_nr.to_be();
        packet.header.ack_nr = self.ack_nr.to_be();
        packet.header.timestamp_microseconds = now_microseconds().to_be();
        packet.set_type(ST_FIN);

        // Send FIN
        let dst = self.connected_to;
        try!(self.socket.sendto(packet.bytes().as_slice(), dst));

        // Receive JAKE
        let mut buf = [0u8, ..BUF_SIZE];
        try!(self.socket.recvfrom(buf));
        let resp = UtpPacket::decode(buf);
        assert!(resp.get_type() == ST_STATE);

        // Set socket state
        self.state = CS_CLOSED;

        Ok(())
    }

    /// Receive data from socket.
    ///
    /// On success, returns the number of bytes read and the sender's address.
    pub fn recvfrom(&mut self, buf: &mut[u8]) -> IoResult<(uint,SocketAddr)> {
        use std::cmp::min;

        if self.state == CS_CLOSED {
            use std::io::{IoError, Closed};

            return Err(IoError {
                kind: Closed,
                desc: "Connection closed",
                detail: None,
            });
        }
        let mut b = [0, ..BUF_SIZE];
        let response = self.socket.recvfrom(b);

        let _src: SocketAddr;
        let read;
        match response {
            Ok((nread, src)) => { read = nread; _src = src },
            Err(e) => return Err(e),
        };

        let packet = UtpPacket::decode(b);
        if cfg!(not(test)) { println!("received {}", packet.header) };

        if packet.get_type() == ST_RESET {
            use std::io::{IoError, ConnectionReset};

            return Err(IoError {
                kind: ConnectionReset,
                desc: "Remote host aborted connection (incorrect connection id)",
                detail: None,
            });
        }

        match self.handle_packet(packet) {
            Some(pkt) => {
                let pkt = pkt.wnd_size(BUF_SIZE as u32);
                try!(self.socket.sendto(pkt.bytes().as_slice(), _src));
                if cfg!(not(test)) {
                    println!("sent {}", pkt.header);
                }
            },
            None => {}
        };

        for i in range(0u, min(buf.len(), read-HEADER_SIZE)) {
            buf[i] = b[i+HEADER_SIZE];
        }

        if cfg!(debug) && cfg!(not(test)) {
            println!("payload: {}", Vec::from_slice(buf.slice(0,read-HEADER_SIZE)));
        }

        Ok((read-HEADER_SIZE, _src))
    }

    fn prepare_reply(&self, original: &UtpPacketHeader, t: UtpPacketType) -> UtpPacket {
        let mut resp = UtpPacket::new();
        resp.set_type(t);
        let self_t_micro: u32 = now_microseconds();
        let other_t_micro: u32 = Int::from_be(original.timestamp_microseconds);
        resp.header.timestamp_microseconds = self_t_micro.to_be();
        resp.header.timestamp_difference_microseconds = (self_t_micro - other_t_micro).to_be();
        resp.header.connection_id = self.sender_connection_id.to_be();
        resp.header.seq_nr = self.seq_nr.to_be();
        resp.header.ack_nr = self.ack_nr.to_be();

        resp
    }

    /// TODO: return error on send after connection closed (RST or FIN + all
    /// packets received)
    pub fn sendto(&mut self, buf: &[u8], dst: SocketAddr) -> IoResult<()> {
        let mut packet = UtpPacket::new();
        packet.set_type(ST_DATA);
        packet.payload = Vec::from_slice(buf);
        packet.header.timestamp_microseconds = now_microseconds().to_be();
        packet.header.seq_nr = self.seq_nr.to_be();
        packet.header.ack_nr = self.ack_nr.to_be();
        packet.header.connection_id = self.sender_connection_id.to_be();

        let r = self.socket.sendto(packet.bytes().as_slice(), dst);

        // Expect ACK
        let mut buf = [0, ..BUF_SIZE];
        try!(self.socket.recvfrom(buf));
        let resp = UtpPacket::decode(buf);
        println!("received {}", resp.header);
        assert_eq!(resp.get_type(), ST_STATE);
        assert_eq!(Int::from_be(resp.header.ack_nr), self.seq_nr);

        // Success, increment sequence number
        if buf.len() > 0 {
            self.seq_nr += 1;
        }

        r
    }

    /// Handle incoming packet, updating socket state accordingly.
    ///
    /// Returns appropriate reply packet, if needed.
    fn handle_packet(&mut self, packet: UtpPacket) -> Option<UtpPacket> {
        // Reset connection if connection id doesn't match and this isn't a SYN
        if packet.get_type() != ST_SYN &&
           !(Int::from_be(packet.header.connection_id) == self.sender_connection_id ||
           Int::from_be(packet.header.connection_id) == self.receiver_connection_id) {
            return Some(self.prepare_reply(&packet.header, ST_RESET));
        }

        self.ack_nr = Int::from_be(packet.header.seq_nr);

        match packet.header.get_type() {
            ST_SYN => { // Respond with an ACK and populate own fields
                // Update socket information for new connections
                self.seq_nr = random();
                self.receiver_connection_id = Int::from_be(packet.header.connection_id) + 1;
                self.sender_connection_id = Int::from_be(packet.header.connection_id);
                self.state = CS_CONNECTED;

                Some(self.prepare_reply(&packet.header, ST_STATE))
            }
            ST_DATA => Some(self.prepare_reply(&packet.header, ST_STATE)),
            ST_FIN => {
                self.state = CS_FIN_RECEIVED;
                // TODO: check if no packets are missing
                // If all packets are received
                self.state = CS_CLOSED;
                Some(self.prepare_reply(&packet.header, ST_STATE))
            }
            ST_STATE => None,
            ST_RESET => /* TODO */ None,
        }
    }
}

impl Clone for UtpSocket {
    fn clone(&self) -> UtpSocket {
        UtpSocket {
            socket: self.socket.clone(),
            connected_to: self.connected_to,
            receiver_connection_id: self.receiver_connection_id,
            sender_connection_id: self.sender_connection_id,
            seq_nr: self.seq_nr,
            ack_nr: self.ack_nr,
            state: self.state,
        }
    }
}

/// Stream interface for UtpSocket.
pub struct UtpStream {
    socket: UtpSocket,
}

impl UtpStream {
    pub fn connect(dst: SocketAddr) -> UtpStream {
        use std::io::net::ip::{Ipv4Addr};
        use std::rand::random;

        let my_addr = SocketAddr { ip: Ipv4Addr(127,0,0,1), port: random() };
        let socket = UtpSocket::bind(my_addr).unwrap().connect(dst);
        UtpStream { socket: socket }
    }

    pub fn close(&mut self) -> IoResult<()> {
        self.socket.close()
    }
}

impl Reader for UtpStream {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<uint> {
        match self.socket.recvfrom(buf) {
            Ok((read, _src)) => Ok(read),
            Err(e) => Err(e),
        }
    }
}

impl Writer for UtpStream {
    fn write(&mut self, buf: &[u8]) -> IoResult<()> {
        let dst = self.socket.connected_to;
        self.socket.sendto(buf, dst)
    }
}

#[cfg(test)]
mod test {
    use super::{UtpSocket, UtpPacket};
    use super::{ST_STATE, ST_FIN, ST_DATA, ST_RESET, ST_SYN};
    use super::{BUF_SIZE, HEADER_SIZE};
    use super::{CS_CONNECTED, CS_NEW, CS_CLOSED};
    use std::rand::random;

    macro_rules! expect_eq(
        ($left:expr, $right:expr) => (
            if !($left == $right) {
                fail!("expected {}, got {}", $right, $left);
            }
        );
    )

    #[test]
    fn test_packet_decode() {
        let buf = [0x21, 0x00, 0x41, 0xa8, 0x99, 0x2f, 0xd0, 0x2a, 0x9f, 0x4a,
                    0x26, 0x21, 0x00, 0x10, 0x00, 0x00, 0x3a, 0xf2, 0x6c, 0x79];
        let pkt = UtpPacket::decode(buf);
        assert_eq!(pkt.header.get_version(), 1);
        assert_eq!(pkt.header.get_type(), ST_STATE);
        assert_eq!(pkt.header.extension, 0);
        assert_eq!(Int::from_be(pkt.header.connection_id), 16808);
        assert_eq!(Int::from_be(pkt.header.timestamp_microseconds), 2570047530);
        assert_eq!(Int::from_be(pkt.header.timestamp_difference_microseconds), 2672436769);
        assert_eq!(Int::from_be(pkt.header.wnd_size), ::std::num::pow(2u32, 20));
        assert_eq!(Int::from_be(pkt.header.seq_nr), 15090);
        assert_eq!(Int::from_be(pkt.header.ack_nr), 27769);
        assert_eq!(pkt.len(), buf.len());
        assert!(pkt.payload.is_empty());
    }

    #[test]
    fn test_packet_encode() {
        let payload = Vec::from_slice("Hello\n".as_bytes());
        let (timestamp, timestamp_diff): (u32, u32) = (15270793, 1707040186);
        let (connection_id, seq_nr, ack_nr): (u16, u16, u16) = (16808, 15090, 17096);
        let window_size: u32 = 1048576;
        let mut pkt = UtpPacket::new();
        pkt.set_type(ST_DATA);
        pkt.header.timestamp_microseconds = timestamp.to_be();
        pkt.header.timestamp_difference_microseconds = timestamp_diff.to_be();
        pkt.header.connection_id = connection_id.to_be();
        pkt.header.seq_nr = seq_nr.to_be();
        pkt.header.ack_nr = ack_nr.to_be();
        pkt.header.wnd_size = window_size.to_be();
        pkt.payload = payload.clone();
        let header = pkt.header;
        let buf: &[u8] = [0x01, 0x00, 0x41, 0xa8, 0x00, 0xe9, 0x03, 0x89,
                    0x65, 0xbf, 0x5d, 0xba, 0x00, 0x10, 0x00, 0x00,
                    0x3a, 0xf2, 0x42, 0xc8, 0x48, 0x65, 0x6c, 0x6c,
                    0x6f, 0x0a];

        assert_eq!(pkt.len(), buf.len());
        assert_eq!(pkt.len(), HEADER_SIZE + payload.len());
        assert_eq!(pkt.payload, payload);
        assert_eq!(header.get_version(), 1);
        assert_eq!(header.get_type(), ST_DATA);
        assert_eq!(header.extension, 0);
        assert_eq!(Int::from_be(header.connection_id), connection_id);
        assert_eq!(Int::from_be(header.seq_nr), seq_nr);
        assert_eq!(Int::from_be(header.ack_nr), ack_nr);
        assert_eq!(Int::from_be(header.wnd_size), window_size);
        assert_eq!(Int::from_be(header.timestamp_microseconds), timestamp);
        assert_eq!(Int::from_be(header.timestamp_difference_microseconds), timestamp_diff);
        assert_eq!(pkt.bytes(), Vec::from_slice(buf));
    }

    #[test]
    fn test_reversible() {
        let buf: &[u8] = [0x01, 0x00, 0x41, 0xa8, 0x00, 0xe9, 0x03, 0x89,
                    0x65, 0xbf, 0x5d, 0xba, 0x00, 0x10, 0x00, 0x00,
                    0x3a, 0xf2, 0x42, 0xc8, 0x48, 0x65, 0x6c, 0x6c,
                    0x6f, 0x0a];
        assert_eq!(UtpPacket::decode(buf).bytes().as_slice(), buf);
    }

    #[test]
    fn test_socket_ipv4() {
        use std::io::test::next_test_ip4;

        let (serverAddr, clientAddr) = (next_test_ip4(), next_test_ip4());

        let client = UtpSocket::bind(clientAddr).unwrap();
        let mut server = UtpSocket::bind(serverAddr).unwrap();

        assert!(server.state == CS_NEW);
        assert!(client.state == CS_NEW);

        // Check proper difference in client's send connection id and receive connection id
        assert_eq!(client.sender_connection_id, client.receiver_connection_id + 1);

        spawn(proc() {
            let client = client.connect(serverAddr);
            assert!(client.state == CS_CONNECTED);
            drop(client);
        });

        let mut buf = [0u8, ..BUF_SIZE];
        match server.recvfrom(buf) {
            e => println!("{}", e),
        }
        // After establishing a new connection, the server's ids are a mirror of the client's.
        assert_eq!(server.receiver_connection_id, server.sender_connection_id + 1);

        assert!(server.state == CS_CONNECTED);
        drop(server);
    }

    #[test]
    fn test_recvfrom_on_closed_socket() {
        use std::io::test::next_test_ip4;
        use std::io::Closed;

        let (serverAddr, clientAddr) = (next_test_ip4(), next_test_ip4());

        let client = UtpSocket::bind(clientAddr).unwrap();
        let mut server = UtpSocket::bind(serverAddr).unwrap();

        assert!(server.state == CS_NEW);
        assert!(client.state == CS_NEW);

        spawn(proc() {
            let mut client = client.connect(serverAddr);
            assert!(client.state == CS_CONNECTED);
            assert_eq!(client.close(), Ok(()));
            drop(client);
        });

        // Make the server listen for incoming connections
        let mut buf = [0u8, ..BUF_SIZE];
        let _resp = server.recvfrom(buf);
        assert!(server.state == CS_CONNECTED);

        // Closing the connection is fine
        match server.recvfrom(buf) {
            Err(e) => fail!("{}", e),
            _ => {},
        }
        expect_eq!(server.state, CS_CLOSED);

        // Trying to listen on the socket after closing it raises an error
        match server.recvfrom(buf) {
            Err(e) => expect_eq!(e.kind, Closed),
            v => fail!("expected {}, got {}", Closed, v),
        }

        drop(server);
    }

    #[test]
    fn test_handle_packet() {
        use std::io::test::next_test_ip4;

        //fn test_connection_setup() {
        let initial_connection_id: u16 = random();
        let sender_connection_id = initial_connection_id + 1;
        let serverAddr = next_test_ip4();
        let mut socket = UtpSocket::bind(serverAddr).unwrap();

        let mut packet = UtpPacket::new().wnd_size(BUF_SIZE as u32);
        packet.set_type(ST_SYN);
        packet.header.connection_id = initial_connection_id.to_be();
        let sent = packet.header;

        // Do we have a response?
        let response = socket.handle_packet(packet.clone());
        assert!(response.is_some());

        // Is is of the correct type?
        let response = response.unwrap();
        assert!(response.get_type() == ST_STATE);

        // Same connection id on both ends during connection establishment
        assert!(response.header.connection_id == sent.connection_id);

        // Response acknowledges SYN
        assert!(response.header.ack_nr == sent.seq_nr);

        // No payload?
        assert!(response.payload.is_empty());
        //}

        // ---------------------------------

        // fn test_connection_usage() {
        let old_packet = packet;
        let old_response = response;

        let mut packet = UtpPacket::new();
        packet.set_type(ST_DATA);
        packet.header.connection_id = sender_connection_id.to_be();
        packet.header.seq_nr = (Int::from_be(old_packet.header.seq_nr) + 1).to_be();
        packet.header.ack_nr = old_response.header.seq_nr;
        let sent = packet.header;

        let response = socket.handle_packet(packet.clone());
        assert!(response.is_some());

        let response = response.unwrap();
        assert!(response.get_type() == ST_STATE);

        // Sender (i.e., who initated connection and sent SYN) has connection id
        // equal to initial connection id + 1
        // Receiver (i.e., who accepted connection) has connection id equal to
        // initial connection id
        assert!(Int::from_be(response.header.connection_id) == initial_connection_id);
        assert!(Int::from_be(response.header.connection_id) == Int::from_be(sent.connection_id) - 1);

        // Previous packets should be ack'ed
        assert!(Int::from_be(response.header.ack_nr) == Int::from_be(sent.seq_nr));

        // Responses with no payload should not increase the sequence number
        assert!(response.payload.is_empty());
        assert!(Int::from_be(response.header.seq_nr) == Int::from_be(old_response.header.seq_nr));
        // }

        //fn test_connection_teardown() {
        let old_packet = packet;
        let old_response = response;

        let mut packet = UtpPacket::new();
        packet.set_type(ST_FIN);
        packet.header.connection_id = sender_connection_id.to_be();
        packet.header.seq_nr = (Int::from_be(old_packet.header.seq_nr) + 1).to_be();
        packet.header.ack_nr = old_response.header.seq_nr;
        let sent = packet.header;

        let response = socket.handle_packet(packet);
        assert!(response.is_some());

        let response = response.unwrap();

        assert!(response.get_type() == ST_STATE);

        // FIN packets have no payload but the sequence number shouldn't increase
        assert!(Int::from_be(sent.seq_nr) == Int::from_be(old_packet.header.seq_nr) + 1);

        // Nor should the ACK packet's sequence number
        assert!(response.header.seq_nr == old_response.header.seq_nr);

        // FIN should be acknowledged
        assert!(response.header.ack_nr == sent.seq_nr);

        //}
    }

    #[test]
    fn test_response_to_keepalive_ack() {
        use std::io::test::next_test_ip4;

        // Boilerplate test setup
        let initial_connection_id: u16 = random();
        let serverAddr = next_test_ip4();
        let mut socket = UtpSocket::bind(serverAddr).unwrap();

        // Establish connection
        let mut packet = UtpPacket::new().wnd_size(BUF_SIZE as u32);
        packet.set_type(ST_SYN);
        packet.header.connection_id = initial_connection_id.to_be();

        let response = socket.handle_packet(packet.clone());
        assert!(response.is_some());
        let response = response.unwrap();
        assert!(response.get_type() == ST_STATE);

        let old_packet = packet;
        let old_response = response;

        // Now, send a keepalive packet
        let mut packet = UtpPacket::new().wnd_size(BUF_SIZE as u32);
        packet.set_type(ST_STATE);
        packet.header.connection_id = initial_connection_id.to_be();
        packet.header.seq_nr = (Int::from_be(old_packet.header.seq_nr) + 1).to_be();
        packet.header.ack_nr = old_response.header.seq_nr;

        let response = socket.handle_packet(packet.clone());
        assert!(response.is_none());

        // Send a second keepalive packet, identical to the previous one
        let response = socket.handle_packet(packet.clone());
        assert!(response.is_none());
    }

    #[test]
    fn test_response_to_wrong_connection_id() {
        use std::io::test::next_test_ip4;

        // Boilerplate test setup
        let initial_connection_id: u16 = random();
        let serverAddr = next_test_ip4();
        let mut socket = UtpSocket::bind(serverAddr).unwrap();

        // Establish connection
        let mut packet = UtpPacket::new().wnd_size(BUF_SIZE as u32);
        packet.set_type(ST_SYN);
        packet.header.connection_id = initial_connection_id.to_be();

        let response = socket.handle_packet(packet.clone());
        assert!(response.is_some());
        assert!(response.unwrap().get_type() == ST_STATE);

        // Now, disrupt connection with a packet with an incorrect connection id
        let new_connection_id = initial_connection_id.to_le();

        let mut packet = UtpPacket::new().wnd_size(BUF_SIZE as u32);
        packet.set_type(ST_STATE);
        packet.header.connection_id = new_connection_id;

        let response = socket.handle_packet(packet.clone());
        assert!(response.is_some());

        let response = response.unwrap();
        assert!(response.get_type() == ST_RESET);
        assert!(response.header.ack_nr == packet.header.seq_nr);
    }
}
