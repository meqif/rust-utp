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
            let header_size = 20;
            return header_size;
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
        skt: UdpSocket,
        seq_nr: u16,
    }

    impl UtpSocket {
        pub fn bind(addr: SocketAddr) -> IoResult<UtpSocket> {
            let skt = UdpSocket::bind(addr);
            match skt {
                Ok(x)  => Ok(UtpSocket { skt: x, seq_nr: 1 }),
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
            let result = self.skt.sendto(packet.bytes().as_slice(), dst);

            self.seq_nr += 1;
            result
        }

        pub fn recvfrom(&mut self, buf: &mut[u8]) -> IoResult<(uint,SocketAddr)> {
            self.skt.recvfrom(buf)
        }
    }
}

fn main() {
    let mut buf = [0, ..512];
    let mut sock = libtorresmo::UtpSocket::bind(SocketAddr { ip: Ipv4Addr(127,0,0,1), port: 8080 }).unwrap();

    match sock.recvfrom(buf) {
        Ok((_, src)) => {
            let payload = String::from_str("Hello\n").into_bytes();

            // Send uTP packet
            let _ = sock.sendto(payload.as_slice(), src);
        }
        Err(_) => {}
    }
    drop(sock);
}
