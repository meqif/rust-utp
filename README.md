# rust-utp

A [Micro Transport Protocol](http://www.bittorrent.org/beps/bep_0029.html) library implemented in Rust.

Currently only the bare minimum is implemented, lacking both congestion control
and lost packet handling, as well as the Selective ACK extension.

## To implement

- [ ] congestion control
- [ ] proper connection closing
    - [x] handle both RST and FIN
    - [x] send FIN on close
    - [ ] automatically send FIN (or should it be RST?) on drop if not already closed
- [x] sending RST on mismatch
- [ ] setters and getters that hide header field endianness conversion
- [ ] SACK extension
- [ ] handle packet loss
- [x] stream interface

## License

This library is distributed under similar terms to Rust: dual licensed under the MIT license and the Apache license (version 2.0).

See LICENSE-APACHE, LICENSE-MIT, and COPYRIGHT for details.
