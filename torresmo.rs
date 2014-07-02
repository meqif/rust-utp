use std::io::net::ip::{Ipv4Addr, SocketAddr};

mod libtorresmo {
    extern crate time;

    use std::io::net::udp::UdpSocket;
    use std::io::net::ip::SocketAddr;
    use std::io::IoResult;
    use std::mem::{to_be32, to_be16, transmute};
    use std::rand::random;

    #[allow(dead_code,non_camel_case_types)]
    enum UtpPacketType {
        ST_DATA  = 0,
        ST_FIN   = 1,
        ST_STATE = 2,
        ST_RESET = 3,
        ST_SYN   = 4,
    }

    #[allow(dead_code)]
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

        fn bytes(&self) -> Vec<u8> {
            let mut buf: Vec<u8> = Vec::with_capacity(self.len());
            buf.push_all(self.header.bytes());
            buf.push_all(self.payload.as_slice());
            return buf;
        }

        fn len(&self) -> uint {
            self.header.len() + self.payload.len()
        }
    }

    pub struct UtpSocket {
        socket: UdpSocket,
        seq_nr: u16,
    }

    impl UtpSocket {
        pub fn bind(addr: SocketAddr) -> IoResult<UtpSocket> {
            let skt = UdpSocket::bind(addr);
            match skt {
                Ok(x)  => Ok(UtpSocket { socket: x, seq_nr: 1 }),
                Err(e) => Err(e)
            }
        }

        pub fn sendto(&mut self, buf: &[u8], dst: SocketAddr) -> IoResult<()> {
            let mut packet = UtpPacket::new();
            packet.payload = Vec::from_slice(buf);
            packet.header.seq_nr = to_be16(self.seq_nr);
            packet.header.connection_id = to_be16(random());
            let t = time::get_time();
            packet.header.timestamp_microseconds = to_be32((t.sec * 1_000_000) as u32 + (t.nsec/1000) as u32);

            // Send packet
            let result = self.socket.sendto(packet.bytes().as_slice(), dst);

            self.seq_nr += 1;
            result
        }

        pub fn recvfrom(&mut self, buf: &mut[u8]) -> IoResult<(uint,SocketAddr)> {
            self.socket.recvfrom(buf)
        }
    }

    impl Clone for UtpSocket {
        fn clone(&self) -> UtpSocket {
            UtpSocket {
                socket: self.socket.clone(),
                seq_nr: self.seq_nr,
            }
        }
    }

    #[allow(dead_code)]
    struct UtpStream {
        socket: UdpSocket,
        connected_to: SocketAddr,
        seq_nr: u16,
        ack_nr: u16,
    }

    #[allow(dead_code)]
    impl UtpStream {
        fn new(socket: UdpSocket, conn: SocketAddr) -> UtpStream {
            UtpStream {
                socket: socket,
                connected_to: conn,
                seq_nr: 0,
                ack_nr: 0,
            }
        }

        fn disconnect(self) -> UdpSocket {
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
            self.socket.sendto(buf, self.connected_to)
        }
    }
}

fn usage() {
    println!("Usage: torresmo [-s|-c] <address> <port>");
}

fn main() {
    use libtorresmo::{UtpSocket};
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
                            let mut sock = sock;
                            let payload = String::from_str("Hello\n").into_bytes();

                            // Send uTP packet
                            let _ = sock.sendto(payload.as_slice(), src);
                        })
                    }
                    Err(_) => {}
                }
            }
        }
        "-c" => {
            let mut sock = UtpSocket::bind(SocketAddr { ip: Ipv4Addr(127,0,0,1), port: std::rand::random() }).unwrap();
            let _ = sock.sendto([0], addr);
            drop(sock);
        }
        _ => usage(),
    }
}
