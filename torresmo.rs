#![feature(macro_rules)]

//! Implementation of a Micro Transport Protocol library,
//! as well as a small client and server.
//!
//! http://www.bittorrent.org/beps/bep_0029.html

use std::io::net::ip::{Ipv4Addr, SocketAddr};

/// Implementation of a Micro Transport Protocol library.
///
/// http://www.bittorrent.org/beps/bep_0029.html
///
/// TODO
/// ----
///
/// - congestion control
/// - proper connection closing
///     - handle both RST and FIN
///     - send FIN on close
///     - automatically send FIN (or should it be RST?) on drop if not already closed
/// - sending RST on mismatch
/// - setters and getters that hide header field endianness conversion
/// - SACK extension
/// - packet loss
/// - test UtpSocket
pub mod libtorresmo {
    extern crate time;

    use std::io::net::udp::UdpSocket;
    use std::io::net::ip::SocketAddr;
    use std::io::IoResult;
    use std::mem::transmute;
    use std::rand::random;
    use std::fmt;

    static HEADER_SIZE: uint = 20;
    static BUF_SIZE: uint = 4096;

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

        spawn(proc() {
            let client = client.connect(serverAddr);
            assert!(client.state == CS_CONNECTED);
            drop(client);
        });

        let mut buf = [0u8, ..BUF_SIZE];
        match server.recvfrom(buf) {
            e => println!("{}", e),
        }

        assert!(server.state == CS_CONNECTED);
        drop(server);
    }

    #[test]
    fn test_recvfrom_on_closed_socket() {
        use std::io::test::next_test_ip4;
        use std::io::EndOfFile;

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

        let mut buf = [0u8, ..BUF_SIZE];
        let resp = server.recvfrom(buf);
        println!("{}", resp);
        assert!(server.state == CS_CONNECTED);

        match server.recvfrom(buf) {
            Err(e) => fail!("{}", e),
            _ => {},
        }
        assert!(server.state == CS_FIN_RECEIVED);

        match server.recvfrom(buf) {
            Err(e) => assert_eq!(e.kind, EndOfFile),
            _ => fail!("should fail with error"),
        }
        drop(server);
    }

    /// Return current time in microseconds since the UNIX epoch.
    fn now_microseconds() -> u32 {
        let t = time::get_time();
        (t.sec * 1_000_000) as u32 + (t.nsec/1000) as u32
    }

    #[allow(dead_code,non_camel_case_types)]
    #[deriving(PartialEq,Eq)]
    enum UtpPacketType {
        ST_DATA  = 0,
        ST_FIN   = 1,
        ST_STATE = 2,
        ST_RESET = 3,
        ST_SYN   = 4,
    }

    impl fmt::Show for UtpPacketType {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            let s = match *self {
                ST_DATA  => "ST_DATA",
                ST_FIN   => "ST_FIN",
                ST_STATE => "ST_STATE",
                ST_RESET => "ST_RESET",
                ST_SYN   => "ST_SYN",
            };
            write!(f, "{}", s)
        }
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

        fn get_type(self) -> UtpPacketType {
            let t: UtpPacketType = unsafe { transmute(self.type_ver >> 4) };
            t
        }

        fn get_version(self) -> u8 {
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
        /// preseving it.
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

        fn get_type(self) -> UtpPacketType {
            self.header.get_type()
        }

        fn wnd_size(self, new_wnd_size: u32) -> UtpPacket {
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
                payload: self.payload,
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

    #[allow(non_camel_case_types)]
    #[deriving(PartialEq,Eq)]
    enum UtpSocketState {
        CS_NEW,
        CS_CONNECTED,
        CS_SYN_SENT,
        CS_FIN_RECEIVED,
        CS_RST_RECEIVED,
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
                    receiver_connection_id: r.to_be(),
                    sender_connection_id: (r + 1).to_be(),
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

            let mut packet = UtpPacket::new();
            packet.set_type(ST_SYN);
            packet.header.connection_id = (self.sender_connection_id - 1).to_be();
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
            packet.header.connection_id = (self.sender_connection_id - 1).to_be();
            packet.header.seq_nr = self.seq_nr.to_be();
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

            Ok(())
        }

        /// Receive data from socket.
        ///
        /// On success, returns the number of bytes read and the sender's address.
        pub fn recvfrom(&mut self, buf: &mut[u8]) -> IoResult<(uint,SocketAddr)> {
            use std::io::IoError;
            use std::io::{ConnectionReset, EndOfFile};
            match self.state {
                CS_RST_RECEIVED => {
                    return Err(IoError {
                        kind: ConnectionReset,
                        desc: "socket closed with CS_RST_RECEIVED",
                        detail: None,
                    });
                },
                CS_FIN_RECEIVED => {
                    return Err(IoError {
                        kind: EndOfFile,
                        desc: "socket closed with CS_FIN_RECEIVED",
                        detail: None,
                    });
                },
                _ => { /* Pattern here just to shush the compiler */ },
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
            let x = packet.header;
            if cfg!(not(test)) { println!("received {}", packet.header) };
            self.ack_nr = Int::from_be(x.seq_nr);

            match packet.get_type() {
                ST_SYN => { // Respond with an ACK and populate own fields
                    // Update socket information for new connections
                    self.seq_nr = random();
                    self.receiver_connection_id = Int::from_be(x.connection_id) + 1;
                    self.sender_connection_id = Int::from_be(x.connection_id);

                    reply_with_ack!(&x, _src);

                    // Packets with no payload don't increase seq_nr
                    //self.seq_nr += 1;

                    self.state = CS_CONNECTED;
                }
                ST_DATA => reply_with_ack!(&x, _src),
                ST_FIN => {
                    self.state = CS_FIN_RECEIVED;
                    reply_with_ack!(&x, _src);
                }
                _ => {}
            };

            for i in range(0u, ::std::cmp::min(buf.len(), read-HEADER_SIZE)) {
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
            resp.header.timestamp_difference_microseconds = (self_t_micro.to_le() - other_t_micro.to_le()).to_be();
            resp.header.connection_id = self.sender_connection_id.to_be();
            resp.header.seq_nr = self.seq_nr.to_be();
            resp.header.ack_nr = self.ack_nr.to_be();

            resp
        }

        /// TODO: return error on send after connection closed (RST or FIN + all
        /// packets received)
        pub fn sendto(&mut self, buf: &[u8], dst: SocketAddr) -> IoResult<()> {
            match self.state {
                CS_RST_RECEIVED => fail!("socket closed with CS_RST_RECEIVED"),
                CS_FIN_RECEIVED => fail!("socket closed with CS_FIN_RECEIVED"),
                _ => { /* Pattern here just to shush the compiler */ },
            }

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
            let x = resp.header;
            assert_eq!(resp.get_type(), ST_STATE);
            assert_eq!(Int::from_be(x.ack_nr), self.seq_nr);

            // Success, increment sequence number
            if buf.len() > 0 {
                self.seq_nr += 1;
            }

            r
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
}

fn usage() {
    println!("Usage: torresmo [-s|-c] <address> <port>");
}

fn main() {
    use libtorresmo::{UtpStream, UtpSocket};
    use std::from_str::FromStr;

    // Defaults
    static BUF_SIZE: uint = 4096;
    let mut addr = SocketAddr { ip: Ipv4Addr(127,0,0,1), port: 8080 };

    let args = std::os::args();

    if args.len() == 4 {
        addr = SocketAddr {
            ip:   FromStr::from_str(args.get(2).as_slice()).unwrap(),
            port: FromStr::from_str(args.get(3).as_slice()).unwrap(),
        };
    }

    match args.get(1).as_slice() {
        "-s" => {
            let mut buf = [0, ..BUF_SIZE];
            let mut sock = UtpSocket::bind(addr).unwrap();
            println!("Serving on {}", addr);

            loop {
                match sock.recvfrom(buf) {
                    Ok((read, _src)) => {
                        spawn(proc() {
                            let buf = buf.slice(0, read);
                            let path = Path::new("output.txt");
                            let mut file = match std::io::File::open_mode(&path, std::io::Open, std::io::ReadWrite) {
                                Ok(f) => f,
                                Err(e) => fail!("file error: {}", e),
                            };
                            match file.write(buf) {
                                Ok(_) => {}
                                Err(e) => fail!("error writing to file: {}", e),
                            };
                            /*
                            let mut stream = sock.connect(src);
                            let payload = String::from_str("Hello\n").into_bytes();

                            // Send uTP packet
                            let _ = stream.write(payload.as_slice());
                            */
                        })
                    }
                    Err(e) => { println!("{}", e); break; }
                }
            }
        }
        "-c" => {
            let mut stream = UtpStream::connect(addr);

            let buf = [0xF, ..BUF_SIZE];
            match stream.write(buf) {
                Ok(_) => {},
                Err(e) => fail!("{}", e),
            }

            let mut buf = [0u8, ..BUF_SIZE];
            match stream.read(buf) {
                Ok(n) => println!("{}", Vec::from_slice(buf.slice(0, n))),
                Err(e) => println!("{}", e),
            }

            match stream.close() {
                Ok(_) => {},
                Err(e) => fail!("{}", e),
            }
            drop(stream);
        }
        _ => usage(),
    }
}
