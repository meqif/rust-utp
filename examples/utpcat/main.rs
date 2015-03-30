//! Implementation of a simple uTP client and server.
#![feature(collections,convert,os)]

extern crate utp;

macro_rules! iotry {
    ($e:expr) => (match $e { Ok(v) => v, Err(e) => panic!("{}", e), })
}

fn usage() {
    println!("Usage: utp [-s|-c] <address> <port>");
}

fn main() {
    use utp::UtpStream;
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

    let addr = match (args.next(), args.next()) {
        (None, None) => String::from_str("127.0.0.1:8080"),
        (Some(ip), Some(port)) => format!("{}:{}", ip, port),
        _ => { usage(); return; }
    };
    let addr: &str = addr.as_ref();

    match mode {
        Mode::Server => {
            let mut stream = iotry!(UtpStream::bind(addr));
            let mut writer = stdout();
            let _ = writeln!(&mut stderr(), "Serving on {}", addr);

            let mut payload = vec![0; 1024 * 1024];
            loop {
                match stream.read(&mut payload) {
                    Ok(0) => break,
                    Ok(read) => iotry!(writer.write(&payload[..read])),
                    Err(e) => panic!("{}", e)
                };
            }
        }
        Mode::Client => {
            let mut stream = iotry!(UtpStream::connect(addr));
            let mut reader = stdin();

            let mut payload = vec![0; 1024 * 1024];
            loop {
                match reader.read(&mut payload) {
                    Ok(0) => break,
                    Ok(read) => iotry!(stream.write(&payload[..read])),
                    Err(e) => {
                        iotry!(stream.close());
                        panic!("{:?}", e);
                    }
                };
            }
            iotry!(stream.close());
        }
    }
}
