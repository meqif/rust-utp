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
    use std::io::{stdin, stdout};

    // Defaults
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
            let mut stream = UtpStream::bind(addr);
            let mut writer = stdout();
            println!("Serving on {}", addr);

            let payload = match stream.read_to_end() {
                Ok(v) => v,
                Err(e) => fail!("{}", e),
            };

            match writer.write(payload.as_slice()) {
                Ok(_) => {},
                Err(e) => fail!("{}", e),
            }
        }
        "-c" => {
            let mut stream = UtpStream::connect(addr);
            let mut reader = stdin();

            let payload = match reader.read_to_end() {
                Ok(v) => v,
                Err(e) => fail!("{}", e),
            };

            match stream.write(payload.as_slice()) {
                Ok(_) => {},
                Err(e) => fail!("{}", e),
            };

            match stream.close() {
                Ok(_) => {},
                Err(e) => fail!("{}", e),
            }
            drop(stream);
        }
        _ => usage(),
    }
}
