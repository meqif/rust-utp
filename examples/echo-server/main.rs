extern crate utp;
extern crate env_logger;

use utp::{UtpListener, UtpSocket};
use std::thread;

fn handle_client(mut s: UtpSocket) {
    let mut buf = [0; 1500];

    // Reply to a data packet with its own payload, then end the connection
    match s.recv_from(&mut buf) {
        Ok((nread, src)) => {
            println!("<= [{}] {:?}", src, &buf[..nread]);
            let _ = s.send_to(&buf[..nread]);
        }
        Err(e) => println!("{}", e)
    }
}

fn main() {
    // Start logger
    env_logger::init().unwrap();

    // Create a listener
    let addr = "127.0.0.1:8080";
    let listener = UtpListener::bind(addr).unwrap();

    for socket in listener.incoming() {
        // Spawn a new handler for each new connection
        match socket {
            Ok(socket) => { thread::spawn(move || { handle_client(socket) }); },
            _ => ()
        }
    }
}
