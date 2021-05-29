use std::io::{Read, Write};
use std::thread;
use utp::UtpStream;

macro_rules! iotry {
    ($e:expr) => {
        match $e {
            Ok(e) => e,
            Err(e) => panic!("{}", e),
        }
    };
}

fn next_test_port() -> u16 {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT_OFFSET: AtomicUsize = AtomicUsize::new(0);
    const BASE_PORT: u16 = 9600;
    BASE_PORT + NEXT_OFFSET.fetch_add(1, Ordering::Relaxed) as u16
}

fn next_test_ip4<'a>() -> (&'a str, u16) {
    ("127.0.0.1", next_test_port())
}

fn next_test_ip6<'a>() -> (&'a str, u16) {
    ("::1", next_test_port())
}

#[test]
fn test_stream_open_and_close() {
    let server_addr = next_test_ip4();
    let mut server = iotry!(UtpStream::bind(server_addr));

    let child = thread::spawn(move || {
        let mut client = iotry!(UtpStream::connect(server_addr));
        iotry!(client.close());
        drop(client);
    });

    let mut received = vec![];
    iotry!(server.read_to_end(&mut received));
    iotry!(server.close());
    assert!(child.join().is_ok());
}

#[test]
fn test_stream_open_and_close_ipv6() {
    let server_addr = next_test_ip6();
    let mut server = iotry!(UtpStream::bind(server_addr));

    let child = thread::spawn(move || {
        let mut client = iotry!(UtpStream::connect(server_addr));
        iotry!(client.close());
        drop(client);
    });

    let mut received = vec![];
    iotry!(server.read_to_end(&mut received));
    iotry!(server.close());
    assert!(child.join().is_ok());
}

#[test]
fn test_stream_small_data() {
    // Fits in a packet
    const LEN: usize = 1024;
    let data: Vec<u8> = (0..LEN).map(|idx| idx as u8).collect();
    assert_eq!(LEN, data.len());

    let d = data.clone();
    let server_addr = next_test_ip4();
    let mut server = iotry!(UtpStream::bind(server_addr));

    let child = thread::spawn(move || {
        let mut client = iotry!(UtpStream::connect(server_addr));
        iotry!(client.write(&d[..]));
        iotry!(client.close());
    });

    let mut received = Vec::with_capacity(LEN);
    iotry!(server.read_to_end(&mut received));
    assert!(!received.is_empty());
    assert_eq!(received.len(), data.len());
    assert_eq!(received, data);
    assert!(child.join().is_ok());
}

#[test]
fn test_stream_large_data() {
    // Has to be sent over several packets
    const LEN: usize = 1024 * 1024;
    let data: Vec<u8> = (0..LEN).map(|idx| idx as u8).collect();
    assert_eq!(LEN, data.len());

    let d = data.clone();
    let server_addr = next_test_ip4();
    let mut server = iotry!(UtpStream::bind(server_addr));

    let child = thread::spawn(move || {
        let mut client = iotry!(UtpStream::connect(server_addr));
        iotry!(client.write(&d[..]));
        iotry!(client.close());
    });

    let mut received = Vec::with_capacity(LEN);
    iotry!(server.read_to_end(&mut received));
    assert!(!received.is_empty());
    assert_eq!(received.len(), data.len());
    assert_eq!(received, data);
    assert!(child.join().is_ok());
}

#[test]
fn test_stream_successive_reads() {
    const LEN: usize = 1024;
    let data: Vec<u8> = (0..LEN).map(|idx| idx as u8).collect();
    assert_eq!(LEN, data.len());

    let d = data.clone();
    let server_addr = next_test_ip4();
    let mut server = iotry!(UtpStream::bind(server_addr));

    let child = thread::spawn(move || {
        let mut client = iotry!(UtpStream::connect(server_addr));
        iotry!(client.write(&d[..]));
        iotry!(client.close());
    });

    let mut received = Vec::with_capacity(LEN);
    iotry!(server.read_to_end(&mut received));
    assert!(!received.is_empty());
    assert_eq!(received.len(), data.len());
    assert_eq!(received, data);

    assert_eq!(server.read(&mut received).unwrap(), 0);
    assert!(child.join().is_ok());
}

#[test]
fn test_local_addr() {
    use std::net::ToSocketAddrs;

    let addr = next_test_ip4();
    let addr = addr.to_socket_addrs().unwrap().next().unwrap();
    let stream = UtpStream::bind(addr).unwrap();

    assert!(stream.local_addr().is_ok());
    assert_eq!(stream.local_addr().unwrap(), addr);
}
