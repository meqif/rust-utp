# rust-utp

[![Crate version](http://img.shields.io/badge/crates.io-v0.2.0-blue.svg?style=flat)](https://crates.io/crates/utp)
[![Build Status](http://img.shields.io/travis/meqif/rust-utp.svg?style=flat)](https://travis-ci.org/meqif/rust-utp)

A [Micro Transport Protocol](http://www.bittorrent.org/beps/bep_0029.html)
library implemented in Rust.

The Micro Transport Protocol is a reliable protocol with ordered delivery built
over UDP. Its congestion control algorithm is
[LEDBAT](http://tools.ietf.org/html/rfc6817), which tries to use as much unused
bandwidth as it can but readily yields to competing flows, making it useful for
bulk transfers without introducing congestion in the network.

The current implementation is somewhat incomplete, lacking a complete implementation of congestion
control. However, it does support packet loss detection (except by timeout) the
Selective Acknowledgment extension, handles unordered and duplicate packets and
presents a stream interface (`UtpStream`).

## Building

```sh
git clone https://github.com/meqif/rust-utp.git
cd rust-utp
cargo test
cargo build --release
```

Note that non-release builds are *much* slower.

## Usage

Check the `examples` directory. The simplest example would be:

```rust,no_run
extern crate utp;

use utp::UtpStream;
use std::io::{Read, Write};

fn main() {
    // Connect to an hypothetical local server running on port 8080
    let addr = "127.0.0.1:8080";
    let mut stream = match UtpStream::connect(addr) {
        Ok(stream) => stream,
        Err(e) => panic!("{}", e),
    };

    // Send a string
    match stream.write("Hi there!".as_bytes()) {
        Ok(_) => (),
        Err(e) => println!("Write failed with {}", e)
    }

    // Close the stream
    match stream.close() {
        Ok(()) => println!("Connection closed"),
        Err(e) => println!("{}", e)
    }
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
