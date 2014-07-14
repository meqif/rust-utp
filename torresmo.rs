use std::io::net::ip::{Ipv4Addr, SocketAddr};

mod libtorresmo {
    extern crate time;

    use std::io::net::udp::UdpSocket;
    use std::io::net::ip::SocketAddr;
    use std::io::IoResult;
    use std::mem::transmute;
    use std::rand::random;
    use std::fmt;

    static HEADER_SIZE: uint = 20;

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
            write!(f, "(type: {}, version: {}, extension: {}, connection_id: {}, timestamp_microseconds: {}, timestamp_difference_microseconds: {}, wnd_size: {}, seq_nr: {}, ack_nr: {})", self.get_type(), Int::from_be(self.get_version()), Int::from_be(self.extension), Int::from_be(self.connection_id), Int::from_be(self.timestamp_microseconds), Int::from_be(self.timestamp_difference_microseconds), Int::from_be(self.wnd_size), Int::from_be(self.seq_nr), Int::from_be(self.ack_nr))
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

        fn bytes(&self) -> Vec<u8> {
            let mut buf: Vec<u8> = Vec::with_capacity(self.len());
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
    }

    pub struct UtpSocket {
        socket: UdpSocket,
        connected_to: SocketAddr,
        sender_connection_id: u16,
        receiver_connection_id: u16,
        seq_nr: u16,
        ack_nr: u16,
        state: UtpSocketState,
    }

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

        pub fn connect(mut self, other: SocketAddr) -> UtpSocket {
            self.connected_to = other;
            /*
            let stream = UtpStream::new(s, other);
            stream.connect()
            */
            let mut packet = UtpPacket::new();
            packet.set_type(ST_SYN);
            packet.header.connection_id = (self.sender_connection_id - 1).to_be();
            packet.header.seq_nr = self.seq_nr.to_be();
            let t = time::get_time();
            packet.header.timestamp_microseconds = ((t.sec * 1_000_000) as u32 + (t.nsec/1000) as u32).to_be();

            // Send packet
            let dst = self.connected_to;
            let _result = self.socket.sendto(packet.bytes().as_slice(), dst);
            println!("sent {}", packet.header);

            self.state = CS_SYN_SENT;

            let mut buf = [0, ..512];
            let (_len, addr) = self.recvfrom(buf).unwrap();
            println!("Connected to: {} {}", addr, self.connected_to);
            assert!(addr == self.connected_to);

            self.seq_nr += 1;

            self
        }

        pub fn recvfrom(&mut self, buf: &mut[u8]) -> IoResult<(uint,SocketAddr)> {
            let mut b = [0, ..512];
            let response = self.socket.recvfrom(b);

            let _src: SocketAddr;
            let read;
            match response {
                Ok((nread, src)) => { read = nread; _src = src },
                Err(e) => return Err(e),
            };

            let packet = UtpPacket::decode(b);
            let x = packet.header;
            println!("received {}", packet.header);
            self.ack_nr = Int::from_be(x.seq_nr);

            match packet.get_type() {
                ST_SYN => { // Respond with an ACK and populate own fields
                    self.seq_nr = random();
                    self.receiver_connection_id = Int::from_be(x.connection_id) + 1;
                    self.sender_connection_id = Int::from_be(x.connection_id);

                    let resp = self.prepare_reply(&x, ST_STATE);
                    try!(self.socket.sendto(resp.bytes().as_slice(), _src));
                    println!("sent {}", resp.header);

                    // Packets with no payload don't increase seq_nr
                    //self.seq_nr += 1;

                    self.state = CS_CONNECTED;
                }
                ST_DATA | ST_FIN => { // Respond with ACK
                    let resp = self.prepare_reply(&x, ST_STATE);
                    try!(self.socket.sendto(resp.bytes().as_slice(), _src));
                    println!("sent {}", resp.header);
                }
                _ => {}
            };

            for i in range(0u, ::std::cmp::min(buf.len(), read-HEADER_SIZE)) {
                buf[i] = b[i+HEADER_SIZE];
            }

            println!("{}", Vec::from_slice(buf.slice(0,read-HEADER_SIZE)));

            Ok((read-HEADER_SIZE, _src))
        }

        fn prepare_reply(&self, original: &UtpPacketHeader, t: UtpPacketType) -> UtpPacket {
            let mut resp = UtpPacket::new();
            resp.set_type(t);
            let t = time::get_time();
            let self_t_micro: u32 = (t.sec * 1_000_000) as u32 + (t.nsec/1000) as u32;
            let other_t_micro: u32 = Int::from_be(original.timestamp_microseconds);
            resp.header.timestamp_microseconds = self_t_micro.to_be();
            resp.header.timestamp_difference_microseconds = (self_t_micro.to_le() - other_t_micro.to_le()).to_be();
            resp.header.connection_id = self.sender_connection_id.to_be();
            resp.header.seq_nr = self.seq_nr.to_be();
            resp.header.ack_nr = self.ack_nr.to_be();

            resp
        }

        pub fn sendto(&mut self, buf: &[u8], dst: SocketAddr) -> IoResult<()> {
            let mut packet = UtpPacket::new();
            packet.set_type(ST_DATA);
            packet.payload = Vec::from_slice(buf);
            let t = time::get_time();
            packet.header.timestamp_microseconds = ((t.sec * 1_000_000) as u32 + (t.nsec/1000) as u32).to_be();
            packet.header.seq_nr = self.seq_nr.to_be();
            packet.header.ack_nr = self.ack_nr.to_be();
            packet.header.connection_id = self.sender_connection_id.to_be();

            let r = self.socket.sendto(packet.bytes().as_slice(), dst);

            // Expect ACK
            let mut buf = [0, ..512];
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
            let r: u16 = random();
            UtpSocket {
                socket: self.socket.clone(),
                connected_to: self.connected_to,
                receiver_connection_id: r.to_be(),
                sender_connection_id: (r + 1).to_be(),
                seq_nr: 1,
                ack_nr: 0,
                state: CS_NEW,
            }
        }
    }
}

fn usage() {
    println!("Usage: torresmo [-s|-c] <address> <port>");
}

fn main() {
    use libtorresmo::UtpSocket;
    use std::from_str::FromStr;

    // Defaults
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
            let mut buf = [0, ..512];
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
                            file.write(buf);
                            /*
                            let mut stream = sock.connect(src);
                            let payload = String::from_str("Hello\n").into_bytes();

                            // Send uTP packet
                            let _ = stream.write(payload.as_slice());
                            */
                        })
                    }
                    Err(_) => {}
                }
            }
        }
        "-c" => {
            let sock = UtpSocket::bind(SocketAddr { ip: Ipv4Addr(127,0,0,1), port: std::rand::random() }).unwrap();
            let mut stream = sock.connect(addr);
            let buf = [0xF, ..512];
            let _ = stream.sendto(buf, addr);
            //let _ = stream.write([0]);
            //let mut buf = [0, ..512];
            //stream.read(buf);
            //let stream = stream.terminate();
            drop(stream);
        }
        _ => usage(),
    }
}
