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
    use std::str::FromStr;
    use std::io::{stdin, stdout, stderr};

    enum Mode {
        Server,
        Client
    }
    let mut mode: Mode;

    // Provide a default address
    let mut addr = SocketAddr { ip: Ipv4Addr(127,0,0,1), port: 8080 };

    let args = std::os::args();
    let mut args = args.iter().map(|arg| arg.as_slice());

    // Skip program name
    args.next();

    match args.next() {
        Some("-s") => mode = Mode::Server,
        Some("-c") => mode = Mode::Client,
        _ => { usage(); return; }
    }

    match args.collect::<Vec<_>>().as_slice() {
        [] => (),
        [ip, port] => {
            addr = SocketAddr {
                ip:   FromStr::from_str(ip).expect("Invalid address"),
                port: FromStr::from_str(port).expect("Invalid port"),
            };
        }
        _ => { usage(); return; }
    }

    match mode {
        Mode::Server => {
            let mut stream = UtpStream::bind(addr);
            let mut writer = stdout();
            let _ = writeln!(&mut stderr(), "Serving on {}", addr);

            let payload = iotry!(stream.read_to_end());
            iotry!(writer.write(payload.as_slice()));
        }
        Mode::Client => {
            let mut stream = iotry!(UtpStream::connect(addr));
            let mut reader = stdin();

            let payload = iotry!(reader.read_to_end());
            iotry!(stream.write(payload.as_slice()));
            iotry!(stream.close());
            drop(stream);
        }
    }
}
