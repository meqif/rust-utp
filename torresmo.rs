use std::io::{TcpListener, TcpStream};
use std::io::{Acceptor, Listener};
use std::io::net::udp::UdpSocket;
use std::io::net::ip::{Ipv4Addr, SocketAddr};

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
}

fn connection_handler(stream: TcpStream) {
    let mut sock = stream;
    println!("New client {}", sock.peer_name().unwrap());

    // Read request
    let mut buf = [0, ..512];
    let mut count: uint = 0;

    match sock.read(buf) {
        Err(e) => { println!("Error detected! {}", e) }
        Ok(c) => {
            count = c;
            println!("Received {} bytes:", c);
            println!("{}", std::str::from_utf8(buf).unwrap());
        }
    }

    // Answer request
    match sock.write(buf.slice(0, count)) {
        _ => {}
    }

    println!("Gone!");
    drop(sock);

}

fn main() {
    let header = UtpPacketHeader {
        ver_type: 0 | 0 << 4,
        extension: 0,
        connection_id: 0,
        timestamp_microseconds: 0,
        timestamp_difference_microseconds: 0,
        wnd_size: 0,
        seq_nr: 0,
        ack_nr: 0,
    };

    let mut buf = [0, ..512];
    let mut sock = UdpSocket::bind(SocketAddr { ip: Ipv4Addr(127,0,0,1), port: 8080 }).unwrap();
    match sock.recvfrom(buf) {
        Ok((_, src)) => {
            unsafe {
                let buf: &[u8, ..20] = std::mem::transmute(&header);
                let _ = sock.sendto(*buf, src);
            }
        }
        Err(_) => {}
    }
    drop(sock);
    return;
    // Create socket
    let socket = TcpListener::bind("127.0.0.1", 8080);

    // Listen for new connections
    let mut acceptor = socket.listen();

    // Spawn a new process for handling each incoming connection
    for stream in acceptor.incoming() {
        match stream {
            Err(e) => { println!("{}", e) }
            Ok(stream) => spawn(proc() { connection_handler(stream) })
        }
    }

    drop(acceptor);
}
