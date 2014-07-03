use std::io::net::ip::{Ipv4Addr, SocketAddr};

mod libtorresmo {
    extern crate time;

    use std::io::net::udp::UdpSocket;
    use std::io::net::ip::SocketAddr;
    use std::io::IoResult;
    use std::mem::transmute;
    use std::rand::random;
    use std::fmt;

    #[allow(dead_code,non_camel_case_types)]
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
        fn set_type(&mut self, t: UtpPacketType) {
            let version = 0x0F & self.type_ver;
            self.type_ver = t as u8 << 4 | version;
        }

        fn get_type(self) -> UtpPacketType {
            let t: UtpPacketType = unsafe { transmute(self.type_ver >> 4) };
            t
        }

        fn bytes(&self) -> &[u8] {
            unsafe {
                let buf: &[u8, ..20] = transmute(self);
                return buf.as_slice();
            }
        }

        fn len(&self) -> uint {
            static HEADER_SIZE: uint = 20;
            return HEADER_SIZE;
        }

        fn decode(buf: &[u8]) -> UtpPacketHeader {
            UtpPacketHeader {
                type_ver: buf[0],
                extension: buf[1],
                connection_id: buf[2] as u16 << 8 | buf[3] as u16,
                timestamp_microseconds: buf[4] as u32 << 24 | buf[5] as u32 << 16 | buf[6] as u32 << 8 | buf[7] as u32,
                timestamp_difference_microseconds: buf[8] as u32 << 24 | buf[9] as u32 << 16 | buf[10] as u32 << 8 | buf[11] as u32,
                wnd_size: buf[12] as u32 << 24 | buf[13] as u32 << 16 | buf[14] as u32 << 8 | buf[15] as u32,
                seq_nr: buf[16] as u16 << 8 | buf[17] as u16,
                ack_nr: buf[18] as u16 << 8 | buf[19] as u16,
            }
        }
    }

    impl fmt::Show for UtpPacketHeader {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            let t: UtpPacketType = self.get_type();
            write!(f, "(type: {}, version: {}, extension: {}, connection_id: {}, timestamp_microseconds: {}, timestamp_difference_microseconds: {}, wnd_size: {}, seq_nr: {}, ack_nr: {})", t, self.type_ver & 0x0F, self.extension, self.connection_id, self.timestamp_microseconds, self.timestamp_difference_microseconds, self.wnd_size, self.seq_nr, self.ack_nr)
        }
    }

    #[allow(dead_code)]
    struct UtpPacket {
        header: UtpPacketHeader,
        payload: Vec<u8>,
    }

    impl UtpPacket {
        /// Constructs a new, empty UtpPacket.
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
            let header = UtpPacketHeader::decode(buf);
            UtpPacket { header: header, payload: Vec::from_slice(buf.slice(20, buf.len())) }
        }
    }

    pub struct UtpSocket {
        socket: UdpSocket,
    }

    impl UtpSocket {
        pub fn bind(addr: SocketAddr) -> IoResult<UtpSocket> {
            let skt = UdpSocket::bind(addr);
            match skt {
                Ok(x)  => Ok(UtpSocket { socket: x }),
                Err(e) => Err(e)
            }
        }

        pub fn connect(self, other: SocketAddr) -> UtpStream {
            let stream = UtpStream::new(self.socket, other);
            stream.connect()
        }

        pub fn recvfrom(&mut self, buf: &mut[u8]) -> IoResult<(uint,SocketAddr)> {
            self.socket.recvfrom(buf)
        }
    }

    impl Clone for UtpSocket {
        fn clone(&self) -> UtpSocket {
            UtpSocket {
                socket: self.socket.clone(),
            }
        }
    }

    #[allow(dead_code)]
    pub struct UtpStream {
        socket: UdpSocket,
        connected_to: SocketAddr,
        connection_id: u16,
        seq_nr: u16,
        ack_nr: u16,
    }

    #[allow(dead_code)]
    impl UtpStream {
        pub fn new(socket: UdpSocket, conn: SocketAddr) -> UtpStream {
            let r: u16 = random();
            UtpStream {
                socket: socket,
                connected_to: conn,
                connection_id: r.to_be(),
                seq_nr: 1,
                ack_nr: 0,
            }
        }

        pub fn connect(mut self) -> UtpStream {
            let mut packet = UtpPacket::new();
            packet.set_type(ST_SYN);
            packet.header.connection_id = self.connection_id;
            packet.header.seq_nr = self.seq_nr.to_be();
            let t = time::get_time();
            packet.header.timestamp_microseconds = ((t.sec * 1_000_000) as u32 + (t.nsec/1000) as u32).to_be();

            // Send packet
            let _result = self.socket.sendto(packet.bytes().as_slice(), self.connected_to);

            self.seq_nr += 1;

            self
        }

        pub fn terminate(mut self) -> UtpStream {
            let mut packet = UtpPacket::new();
            packet.set_type(ST_FIN);
            packet.header.connection_id = self.connection_id;
            packet.header.seq_nr = self.seq_nr.to_be();
            let t = time::get_time();
            packet.header.timestamp_microseconds = ((t.sec * 1_000_000) as u32 + (t.nsec/1000) as u32).to_be();

            // Send packet
            let _result = self.socket.sendto(packet.bytes().as_slice(), self.connected_to);

            self.seq_nr += 1;

            self
        }

        pub fn disconnect(self) -> UdpSocket {
            self.socket
        }
    }

    impl Reader for UtpStream {
        fn read(&mut self, buf: &mut [u8]) -> IoResult<uint> {
            match self.socket.recvfrom(buf) {
                Ok((_nread, src)) if src != self.connected_to => Ok(0),
                Ok((nread, _src)) => Ok(nread),
                Err(e) => Err(e),
            }
        }
    }
    impl Writer for UtpStream {
        fn write(&mut self, buf: &[u8]) -> IoResult<()> {
            let mut packet = UtpPacket::new();
            packet.payload = Vec::from_slice(buf);
            packet.header.connection_id = self.connection_id;
            packet.header.seq_nr = self.seq_nr.to_be();
            let t = time::get_time();
            packet.header.timestamp_microseconds = ((t.sec * 1_000_000) as u32 + (t.nsec/1000) as u32).to_be();

            // Send packet
            let result = self.socket.sendto(packet.bytes().as_slice(), self.connected_to);

            self.seq_nr += 1;
            result
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
        let ip = FromStr::from_str(args.get(2).as_slice()).unwrap();
        let port = FromStr::from_str(args.get(3).as_slice()).unwrap();
        addr = SocketAddr { ip: ip, port: port };
    }

    match args.get(1).as_slice() {
        "-s" => {
            let mut buf = [0, ..512];
            let sock = UtpSocket::bind(addr).unwrap();
            println!("Serving on {}", addr);

            loop {
                let mut sock = sock.clone();
                match sock.recvfrom(buf) {
                    Ok((_, src)) => {
                        spawn(proc() {
                            let mut stream = sock.connect(src);
                            let payload = String::from_str("Hello\n").into_bytes();

                            // Send uTP packet
                            let _ = stream.write(payload.as_slice());
                        })
                    }
                    Err(_) => {}
                }
            }
        }
        "-c" => {
            let sock = UtpSocket::bind(SocketAddr { ip: Ipv4Addr(127,0,0,1), port: std::rand::random() }).unwrap();
            let mut stream = sock.connect(addr);
            let _ = stream.write([0]);
            let stream = stream.terminate();
            drop(stream);
        }
        _ => usage(),
    }
}
