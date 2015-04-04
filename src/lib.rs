//! Implementation of the Micro Transport Protocol.[^spec]
//!
//! [^spec]: http://www.bittorrent.org/beps/bep_0029.html

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

//   __________  ____  ____
//  /_  __/ __ \/ __ \/ __ \
//   / / / / / / / / / / / /
//  / / / /_/ / /_/ / /_/ /
// /_/  \____/_____/\____/
//
// - Lossy UDP socket for testing purposes: send and receive ops are wrappers
// that stochastically drop or reorder packets.
// - Sending FIN on drop
// - Handle packet loss
// - Path MTU discovery (RFC4821)

#![deny(missing_docs)]

extern crate rand;
extern crate time;
extern crate num;
#[macro_use] extern crate log;

// Public API
pub use socket::UtpSocket;
pub use stream::UtpStream;

mod util;
mod bit_iterator;
mod packet;
mod socket;
mod stream;
