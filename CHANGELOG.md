# Change Log

## [0.6.3]

### Added

- Added `peer_fn` method to UtpSocket.

### Fixed

- Fixed arithmetic operation overflow when calculating the delay between peers.
- Unconnected sockets and streams no longer send fast resend requests before failing.

## [0.6.2]

### Fixed

- Fixed Windows build.

## [0.6.1]

### Added

- Packets are resent if no acknowledgment is received before timeout.
- Sockets time out after too many retransmissions (configurable), returning an error.

### Fixed

- Socket creation no longer fails with arithmetic overflow when generating a
  random connection identifier.
- SACK extension generation no longer fails with arithmetic overflow.
- Resending lost packets no longer floods the connection.
- Fixed packet extension encoding.
- Many protocol bug fixes.
- Fixed warning about Sized in trait on rustc 1.4.0-nightly (10d69db0a 2015-08-23) (RFC 1214 fallout)

## [0.6.0]

### Added

- Implemented the `Deref` trait for easy conversion from `UtpStream` instances into the underlying `UtpSocket`.

### Changed

- `UtpSocket::connect` is now a function instead of a method on a `UtpSocket` instance (i.e., it doesn't require a socket to be called).

## [0.5.1]

### Added

- Added `local_addr` for both `UtpSocket` and `UtpStream`.

## [0.5.0]

### Added

- Added `local_addr` for both `UtpSocket` and `UtpStream`.
- Added the `Into` trait for easy conversion from `UtpSocket` instances into `UtpStream`.

### Changed

- `UtpListener::accept` now returns both the new socket and the remote peer's address (`Result<UtpSocket, SocketAddr>`), similarly to `TcpListener`.
- `UtpListener::incoming` now also returns the remote peer's address, similarly to `accept` but unlike `TcpListener::incoming`.

## [0.4.0]

### Added

- Added `UtpListener` (similar to [`TcpListener`][http://doc.rust-lang.org/std/net/struct.TcpListener.html]).

## [0.3.1]

### Fixed

- Removed assertions about `off_target`, which were killing innocent connections.

## [0.3.0]

### Fixed

- Fixed bug when adjusting congestion window caused by miscalculating the delay between peers.
- Fixed bug where missing packets weren't being re-sent after sending a FIN.
- Fixed bug where a stream wouldn't bind to an address of the appropriate family when the remote peer had an IPv6 address.
- Fixed bug where the congestion window would only shrink when packet loss was detected and not on delay changes.

### Changed

- A call to `UtpStream::write` or `UtpSocket::send_to` no longer blocks until every packet is acknowledged. To force the old, slower behaviour, call `flush` after the usual calls (usually you won't need to do this, as the socket/stream is flushed on close/drop).

## [0.2.8]

### Fixed

- Fixed bug where extensions could be skipped when parsing a packet.
- Improved reliability of packet parsing.

## [0.2.7]

### Fixed

- Fixed compilation errors in 1.0.0-beta.2

### Changed

- Improved resilience to errors --- receiving an invalid packet no longer leads to a panic
- Sockets with established connections refuse new connections

## [0.2.6]

No functional changes.

### Changed

- Removed stability attributes.
- Removed an unnecessary partial clone when handling a received packet.

## [0.2.5]

### Changed

- Dropping an `UtpSocket` (or a wrapping struct like `UtpStream`) properly closes open connections.

## [0.2.4]

Improved performance encoding and decoding packets.

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
