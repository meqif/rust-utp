# rust-utp

A [Micro Transport Protocol](http://www.bittorrent.org/beps/bep_0029.html) library implemented in Rust.

The current implementation is somewhat incomplete, lacking both congestion
control and full packet loss handling (though some cases are handled). However,
it does support the Selective Acknowledgment extension, handles unordered and
duplicate packets and presents a stream interface (`UtpStream`).

[![Build Status](http://img.shields.io/travis/meqif/rust-utp.svg?style=flat)](https://travis-ci.org/meqif/rust-utp)

## Building

```
git clone https://github.com/meqif/rust-utp.git
cd rust-utp
cargo test
cargo build --release
```

Note that non-release builds are *much* slower.

## Usage

Check the `examples` directory. The simplest example would be:

```rust
extern crate utp;

use utp::UtpStream;
use std::io::net::ip::{Ipv4Addr, SocketAddr};

fn main() {
    let addr = SocketAddr { ip: Ipv4Addr(127,0,0,1), port: 8080 };

    let mut stream = match UtpStream::connect(addr) {
        Ok(stream) => stream,
        Err(e) => fail!("{}", e),
    };

    // Ignoring the result of both calls below for the sake of brevity
    stream.write("Hi there!".as_bytes());
    stream.close();
}
```

## To implement

- [x] congestion control
- [ ] proper connection closing
    - [x] handle both RST and FIN
    - [x] send FIN on close
    - [ ] automatically send FIN (or should it be RST?) on `drop` if not already closed
- [x] sending RST on mismatch
- [x] setters and getters that hide header field endianness conversion
- [x] SACK extension
- [ ] handle packet loss
    - [x] send triple-ACK to re-request lost packet (fast resend request)
    - [x] rewind send window and resend in reply to triple-ACK (fast resend)
    - [ ] resend packet on ACK timeout
- [x] stream interface
- [x] handle unordered packets
- [ ] path MTU discovery
- [x] duplicate packet handling

## License

This library is distributed under similar terms to Rust: dual licensed under the MIT license and the Apache license (version 2.0).

See LICENSE-APACHE, LICENSE-MIT, and COPYRIGHT for details.
