//! Implementation of a simple uTP client and server.

extern crate utp;
use std::old_io::net::ip::{Ipv4Addr, SocketAddr};

macro_rules! iotry {
    ($e:expr) => (match $e { Ok(v) => v, Err(e) => panic!("{}", e), })
}

fn usage() {
    println!("Usage: utp [-s|-c] <address> <port>");
}

fn main() {
    use utp::UtpStream;
    use std::str::FromStr;
    use std::io::{stdin, stdout, stderr, Read, Write};

    enum Mode {
        Server,
        Client
    }

    let args = std::os::args();
    let mut args = args.iter().map(|arg| &arg[..]);

    // Skip program name
    args.next();

    let mode = match args.next() {
        Some("-s") => Mode::Server,
        Some("-c") => Mode::Client,
        _ => { usage(); return; }
    };

    let addr = match &args.collect::<Vec<_>>()[..] {
        [] => {
            // Use a default address
            SocketAddr { ip: Ipv4Addr(127,0,0,1), port: 8080 }
        },
        [ip, port] => {
            let ip = match FromStr::from_str(ip) {
                Ok(x) => x,
                Err(_) => { println!("Invalid address"); return }
            };
            let port = match FromStr::from_str(port) {
                Ok(x) => x,
                Err(_) => { println!("Invalid port"); return }
            };
            SocketAddr {
                ip:   ip,
                port: port,
            }
        }
        _ => { usage(); return; }
    };

    match mode {
        Mode::Server => {
            let mut stream = UtpStream::bind(addr);
            let mut writer = stdout();
            let _ = writeln!(&mut stderr(), "Serving on {}", addr);

            let payload = iotry!(stream.read_to_end());
            iotry!(writer.write(&payload[..]));
        }
        Mode::Client => {
            let mut stream = iotry!(UtpStream::connect(addr));
            let mut reader = stdin();

            let mut payload = vec!();
            iotry!(reader.read_to_end(&mut payload));
            iotry!(stream.write(&payload[..]));
            iotry!(stream.close());
            drop(stream);
        }
    }
}
