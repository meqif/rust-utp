use std::io::{TcpListener, TcpStream};
use std::io::{Acceptor, Listener};

fn connection_handler(stream: TcpStream) {
    let mut sock = stream;
    println!("New client {}", sock.peer_name().unwrap());

    // Read request
    let mut buf = [0,..512];

    match sock.read(buf) {
        Err(e) => { println!("Error detected! {}", e) }
        Ok(count) => {
            println!("Received {} bytes:", count);
            println!("{}", std::str::from_utf8(buf));
        }
    }

    // Answer request
    match sock.write(buf) {
        _ => {}
    }

    println!("Gone!");
    drop(sock);
}

fn main() {
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
