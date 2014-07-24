#![feature(macro_rules)]

//! Implementation of a Micro Transport Protocol library,
//! as well as a small client and server.
//!
//! http://www.bittorrent.org/beps/bep_0029.html

extern crate utp;
use std::io::net::ip::{Ipv4Addr, SocketAddr};

fn usage() {
    println!("Usage: utp [-s|-c] <address> <port>");
}

fn main() {
    use utp::UtpStream;
    use std::from_str::FromStr;

    // Defaults
    static BUF_SIZE: uint = 4096;
    let mut addr = SocketAddr { ip: Ipv4Addr(127,0,0,1), port: 8080 };

    let args = std::os::args();

	if args.len() != 2 && args.len() != 4 {
		usage();
		return;
	}

    if args.len() == 4 {
        addr = SocketAddr {
            ip:   FromStr::from_str(args[2].as_slice()).unwrap(),
            port: FromStr::from_str(args[3].as_slice()).unwrap(),
        };
    }

    match args[1].as_slice() {
        "-s" => {
            let mut buf = [0, ..BUF_SIZE];
            let mut stream = UtpStream::bind(addr);

            println!("Serving on {}", addr);

            loop {
                match stream.read(buf) {
                    Ok(_read) => {}
                    Err(e) => fail!("{}", e),
                }
            }
        }
        "-c" => {
            let mut stream = UtpStream::connect(addr);

            let buf = [0xF, ..BUF_SIZE];
            match stream.write(buf) {
                Ok(_) => {},
                Err(e) => fail!("{}", e),
            }

            let mut buf = [0u8, ..BUF_SIZE];
            match stream.read(buf) {
                Ok(n) => println!("{}", Vec::from_slice(buf.slice(0, n))),
                Err(e) => println!("{}", e),
            }

            match stream.close() {
                Ok(_) => {},
                Err(e) => fail!("{}", e),
            }
            drop(stream);
        }
        _ => usage(),
    }
}
