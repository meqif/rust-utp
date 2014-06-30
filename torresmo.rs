use std::io::net::udp::UdpSocket;
use std::io::net::ip::{Ipv4Addr, SocketAddr};

#[allow(dead_code)]
struct UtpPacketHeader {
    ver_type: u8, // ver: u4, type: u4
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
            let buf: &[u8, ..20] = std::mem::transmute(self);
            return buf.as_slice();
        }
    }

    fn len(&self) -> uint {
        return 24;
    }
}

#[allow(dead_code)]
struct UtpPacket {
    header: UtpPacketHeader,
    payload: Vec<u8>,
}

impl UtpPacket {
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

fn main() {
    let packet = UtpPacket {
        header: UtpPacketHeader {
            ver_type: 0 | 0 << 4,
            extension: 0,
            connection_id: 0,
            timestamp_microseconds: 0,
            timestamp_difference_microseconds: 0,
            wnd_size: 0,
            seq_nr: 0,
            ack_nr: 0,
        },
        payload: String::from_str("Hello\n").into_bytes(),
    };

    let mut buf = [0, ..512];
    let mut sock = UdpSocket::bind(SocketAddr { ip: Ipv4Addr(127,0,0,1), port: 8080 }).unwrap();
    match sock.recvfrom(buf) {
        Ok((_, src)) => {
            let _ = sock.sendto(packet.bytes().as_slice(), src);
        }
        Err(_) => {}
    }
    drop(sock);
}
