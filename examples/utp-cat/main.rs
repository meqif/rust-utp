//! Implementation of a simple uTP client and server.
#![feature(macro_rules)]

extern crate utp;
use std::io::net::ip::{Ipv4Addr, SocketAddr};

macro_rules! iotry(
    ($e:expr) => (match $e { Ok(v) => v, Err(e) => panic!("{}", e), })
)

fn usage() {
    println!("Usage: utp [-s|-c] <address> <port>");
}

fn main() {
    use utp::UtpStream;
    use std::from_str::FromStr;
    use std::io::{stdin, stdout, stderr};

    // Defaults
    let mut addr = SocketAddr { ip: Ipv4Addr(127,0,0,1), port: 8080 };

    let args = std::os::args();

    if args.len() != 2 && args.len() != 4 {
        usage();
        return;
    }

    if args.len() == 4 {
        addr = SocketAddr {
            ip:   FromStr::from_str(args[2].as_slice()).expect("Invalid address"),
            port: FromStr::from_str(args[3].as_slice()).expect("Invalid port"),
        };
    }

    match args[1].as_slice() {
        "-s" => {
            let mut stream = UtpStream::bind(addr);
            let mut writer = stdout();
            let _ = writeln!(stderr(), "Serving on {}", addr);

            let payload = iotry!(stream.read_to_end());
            iotry!(writer.write(payload.as_slice()));
        }
        "-c" => {
            let mut stream = iotry!(UtpStream::connect(addr));
            let mut reader = stdin();

            let payload = iotry!(reader.read_to_end());
            iotry!(stream.write(payload.as_slice()));
            iotry!(stream.close());
            drop(stream);
        }
        _ => usage(),
    }
}
