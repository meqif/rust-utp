#![feature(macro_rules)]

extern crate utp;

use std::io::test::next_test_ip4;
use utp::UtpStream;

macro_rules! iotry(
    ($e:expr) => (match $e { Ok(e) => e, Err(e) => panic!("{}", e) })
)


#[test]
fn test_stream_open_and_close() {
    let server_addr = next_test_ip4();
    let mut server = iotry!(UtpStream::bind(server_addr));

    spawn(proc() {
        let mut client = iotry!(UtpStream::connect(server_addr));
        iotry!(client.close());
        drop(client);
    });

    iotry!(server.read_to_end());
    iotry!(server.close());
}
