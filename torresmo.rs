use std::io::{TcpListener, TcpStream, BufferedReader};
use std::io::{Acceptor, Listener};

fn main() {
    println!("hello?");

    let socket = TcpListener::bind("127.0.0.1", 8080);

    let mut acceptor = socket.listen();

    for stream in acceptor.incoming() {
        match stream {
            Err(e) => {}
            Ok(stream) => spawn(proc() {
                println!("New client!");
                let mut socket = BufferedReader::new(stream);
                let content = socket.read_to_str();
                println!("{}", content.unwrap());
                println!("Gone!")
            })
        }
    }

    drop(acceptor)
}
