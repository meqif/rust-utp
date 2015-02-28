//! Implementation of the Micro Transport Protocol.[^spec]
//!
//! [^spec]: http://www.bittorrent.org/beps/bep_0029.html

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

extern crate time;
#[macro_use] extern crate log;

// Public API
pub use socket::UtpSocket;
pub use stream::UtpStream;

mod util;
mod bit_iterator;
mod packet;
mod socket;
mod stream;
