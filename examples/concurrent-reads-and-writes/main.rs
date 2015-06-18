extern crate utp;
extern crate env_logger;

use utp::UtpCloneableSocket;
use std::thread;

fn main() {
    // Start logger
    env_logger::init().unwrap();

    let addr = "127.0.0.1:8080";
    let mut socket = UtpCloneableSocket::connect(addr).unwrap();
    let receiving_socket = socket.try_clone();

    thread::spawn(move || {
        let socket = receiving_socket;
        let mut buf = [0; 1500];
        loop {
            match socket.recv(&mut buf) {
                Ok((0, _src)) => break,
                Ok((n, _src)) => println!("<=== received {:?}", &buf[..n]),
                Err(e) => println!("Error in receiver: {:?}", e),
            }
        }
    });

    let mut i = 0;
    loop {
        match socket.send_to(&[i]) {
            Ok(_) => {
                println!("===> sent {}", i);
                i = (i + 1) % 10;
            },
            Err(e) => println!("Error in sender: {:?}", e)
        }
    }
}
