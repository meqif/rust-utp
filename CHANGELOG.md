# Change Log

## [0.2.3]

### Fixed
- Now the crate builds in both the latest nightly and 1.0.0-beta.

## [0.2.2]

No functional changes, mostly just changes to conform to changes in the Rust API.

## [0.2.1]

### Changed
- Updated the `rand` dependency because the previous one didn't build on the latest Rust nightly.

### Fixed
- Some `UtpStream` tests were failing because of improperly sized buffers.

## [0.2.0]

This release is now compatible with the 2015-03-28 nightly of the Rust compiler.
Some things changed during the migration to the new `std::net` API and performance is now much lower. It might take me a while to come up with performance improvements and a replacement for the lost `set_timeout` method in `UdpSocket`.

### Changed
- Updated example in README.
- `UtpStream` and `UtpSocket` now accept variables implementing the `ToSocketAddrs` trait, like `UdpSocket` in the standard library.
- Reading from a socket now returns `Result<(usize, SocketAddr)>`.
- Reading from a stream now returns `Result<usize>`.
- Reading from a closed socket/stream now returns `Ok((0, remote_peer))`/`Ok(0)` instead of `Err(Closed)`.

### Added
- `UtpStream` now implements the `Read` and `Write` traits.

### Removed
- The `Reader` and `Writer` traits were removed, in accordance to the recent IO reform in Rust.
- Support for connection timeouts were removed, which may impact packet loss handling in some cases.
