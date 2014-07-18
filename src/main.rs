#![feature(macro_rules)]

//! Implementation of a Micro Transport Protocol library,
//! as well as a small client and server.
//!
//! http://www.bittorrent.org/beps/bep_0029.html

extern crate utp;
use std::io::net::ip::{Ipv4Addr, SocketAddr};

fn usage() {
    println!("Usage: torresmo [-s|-c] <address> <port>");
}

fn main() {
    use utp::{UtpStream, UtpSocket};
    use std::from_str::FromStr;

    // Defaults
    static BUF_SIZE: uint = 4096;
    let mut addr = SocketAddr { ip: Ipv4Addr(127,0,0,1), port: 8080 };

    let args = std::os::args();

    if args.len() == 4 {
        addr = SocketAddr {
            ip:   FromStr::from_str(args.get(2).as_slice()).unwrap(),
            port: FromStr::from_str(args.get(3).as_slice()).unwrap(),
        };
    }

    match args.get(1).as_slice() {
        "-s" => {
            let mut buf = [0, ..BUF_SIZE];
            let mut sock = UtpSocket::bind(addr).unwrap();
            println!("Serving on {}", addr);

            loop {
                match sock.recvfrom(buf) {
                    Ok((read, _src)) => {
                        spawn(proc() {
                            let buf = buf.slice(0, read);
                            let path = Path::new("output.txt");
                            let mut file = match std::io::File::open_mode(&path, std::io::Open, std::io::ReadWrite) {
                                Ok(f) => f,
                                Err(e) => fail!("file error: {}", e),
                            };
                            match file.write(buf) {
                                Ok(_) => {}
                                Err(e) => fail!("error writing to file: {}", e),
                            };
                        })
                    }
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
