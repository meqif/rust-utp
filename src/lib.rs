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
//! use std::io::Write;
//!
//! fn main() {
//!     // Connect to an hypothetical local server running on port 8080
//!     let addr = "127.0.0.1:8080";
//!     let mut stream = UtpStream::connect(addr).expect("Error connecting to remote peer");
//!
//!     // Send a string
//!     stream.write("Hi there!".as_bytes()).expect("Write failed");
//!
//!     // Close the stream
//!     stream.close().expect("Error closing connection");
//! }
//! ```
//!
//! Note that you can easily convert a socket to a stream using the `Into` trait, like so:
//!
//! ```no_run
//! # use utp::{UtpStream, UtpSocket};
//! let socket = UtpSocket::bind("0.0.0.0:0").expect("Error binding socket");
//! let stream: UtpStream = socket.into();
//! ```

#![deny(missing_docs)]

// Optional features
#![cfg_attr(feature = "clippy", feature(plugin))]
#![cfg_attr(feature = "clippy", plugin(clippy))]
#![cfg_attr(feature = "clippy", allow(len_without_is_empty, doc_markdown, needless_return,
                                      transmute_ptr_to_ref))]
#![cfg_attr(feature = "unstable", feature(test))]

extern crate rand;
extern crate num_traits;
#[macro_use] extern crate log;
#[cfg(test)] extern crate quickcheck;

// Public API
pub use socket::UtpSocket;
pub use socket::UtpListener;
pub use stream::UtpStream;

mod bit_iterator;
mod error;
mod packet;
mod socket;
mod stream;
mod util;
