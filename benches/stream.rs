extern crate test;
extern crate utp;

use test::Bencher;
use std::old_io::test::next_test_ip4;
use utp::UtpStream;
use std::sync::Arc;
use std::thread;

macro_rules! iotry {
    ($e:expr) => (match $e { Ok(e) => e, Err(e) => panic!("{}", e) })
}

#[bench]
fn bench_connection_setup_and_teardown(b: &mut Bencher) {
    let server_addr = next_test_ip4();
    b.iter(|| {
        let mut server = iotry!(UtpStream::bind(server_addr));

        thread::spawn(move || {
            let mut client = iotry!(UtpStream::connect(server_addr));
            iotry!(client.close());
        });

        iotry!(server.read_to_end());
        iotry!(server.close());
    });
}

#[bench]
fn bench_transfer_one_packet(b: &mut Bencher) {
    let len = 1024;
    let server_addr = next_test_ip4();
    let data = (0..len).map(|x| x as u8).collect::<Vec<u8>>();
    let data_arc = Arc::new(data);

    b.iter(|| {
        let data = data_arc.clone();
        let mut server = iotry!(UtpStream::bind(server_addr));

        thread::spawn(move || {
            let mut client = iotry!(UtpStream::connect(server_addr));
            iotry!(client.write(data.as_slice()));
            iotry!(client.close());
        });

        iotry!(server.read_to_end());
        iotry!(server.close());
    });
    b.bytes = len as u64;
}

#[bench]
fn bench_transfer_one_megabyte(b: &mut Bencher) {
    let len = 1024 * 1024;
    let server_addr = next_test_ip4();
    let data = (0..len).map(|x| x as u8).collect::<Vec<u8>>();
    let data_arc = Arc::new(data);

    b.iter(|| {
        let data = data_arc.clone();
        let mut server = iotry!(UtpStream::bind(server_addr));

        thread::spawn(move || {
            let mut client = iotry!(UtpStream::connect(server_addr));
            iotry!(client.write(data.as_slice()));
            iotry!(client.close());
        });

        iotry!(server.read_to_end());
        iotry!(server.close());
    });
    b.bytes = len as u64;
}
