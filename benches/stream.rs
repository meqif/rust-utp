#![feature(macro_rules)]

extern crate test;
extern crate utp;

use test::Bencher;
use std::io::test::next_test_ip4;
use utp::UtpStream;

macro_rules! iotry(
    ($e:expr) => (match $e { Ok(e) => e, Err(e) => panic!("{}", e) })
)

#[bench]
fn bench_connection_setup_and_teardown(b: &mut Bencher) {
    let server_addr = next_test_ip4();
    b.iter(|| {
        let mut server = iotry!(UtpStream::bind(server_addr));

        spawn(proc() {
            let mut client = iotry!(UtpStream::connect(server_addr));
            iotry!(client.close());
        });

        iotry!(server.read_to_end());
        iotry!(server.close());
    });
}

#[bench]
fn bench_transfer_one_megabyte(b: &mut Bencher) {
    let len = 1024 * 1024;
    let server_addr = next_test_ip4();
    b.iter(|| {
        let data = Vec::from_fn(len, |idx| idx as u8);
        let mut server = iotry!(UtpStream::bind(server_addr));

        spawn(proc() {
            let mut client = iotry!(UtpStream::connect(server_addr));
            iotry!(client.write(data.as_slice()));
            iotry!(client.close());
        });

        iotry!(server.read_to_end());
        iotry!(server.close());
    });
}
