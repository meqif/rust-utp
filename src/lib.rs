//! Implementation of the [Micro Transport Protocol][spec].
//!
//! This library provides both a socket interface (`UtpSocket`) and a stream interface
//! (`UtpStream`).
//! I recommend that you use `UtpStream`, as it implements the `Read` and `Write`
//! traits we all know (and love) from `std::io`, which makes it generally easier to work with than
//! `UtpSocket`.
//!
//! [spec]: http://www.bittorrent.org/beps/bep_0029.html
//!
//! # Installation
//!
//! Ensure your `Cargo.toml` contains:
//!
//! ```toml
//! [dependencies]
//! utp = "*"
//! ```
//!
//! # Examples
//!
//! ```no_run
//! extern crate utp;
//!
//! use utp::UtpStream;
//! use std::io::{Read, Write};
//!
//! fn main() {
//!     // Connect to an hypothetical local server running on port 8080
//!     let addr = "127.0.0.1:8080";
//!     let mut stream = match UtpStream::connect(addr) {
//!         Ok(stream) => stream,
//!         Err(e) => panic!("{}", e),
//!     };
//!
//!     // Send a string
//!     match stream.write("Hi there!".as_bytes()) {
//!         Ok(_) => (),
//!         Err(e) => println!("Write failed with {}", e)
//!     }
//!
//!     // Close the stream
//!     match stream.close() {
//!         Ok(()) => println!("Connection closed"),
//!         Err(e) => println!("{}", e)
//!     }
//! }
//! ```
//!
//! Note that you can easily convert a socket to a stream using the `Into` trait, like so:
//!
//! ```no_run
//! # use utp::{UtpStream, UtpSocket};
//! let socket = UtpSocket::bind("0.0.0.0:0").unwrap();
//! let stream: UtpStream = socket.into();
//! ```

#![deny(missing_docs)]

extern crate rand;
extern crate time;
extern crate num;
#[macro_use] extern crate log;
#[cfg(test)] extern crate quickcheck;

#[cfg(unix)]
extern crate nix;
#[cfg(windows)]
extern crate libc;

// Public API
pub use socket::UtpSocket;
pub use socket::UtpListener;
pub use stream::UtpStream;

mod util;
mod bit_iterator;
mod packet;
mod socket;
mod stream;
#[cfg(windows)]
mod select;
mod with_read_timeout;
