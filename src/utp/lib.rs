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
// - Congestion control (LEDBAT -- RFC6817)
// - Sending FIN on drop
// - Setters and getters that hide header field endianness conversion
// - Handle packet loss
// - Path MTU discovery (RFC4821)

#![crate_name = "utp"]

#![license = "MIT/ASL2"]
#![crate_type = "dylib"]
#![crate_type = "rlib"]

#![feature(macro_rules, phase)]
#![deny(missing_doc)]

extern crate time;
#[phase(plugin, link)] extern crate log;

use std::io::net::udp::UdpSocket;
use std::io::net::ip::SocketAddr;
use std::io::IoResult;
use std::mem::transmute;
use std::rand::random;
use std::fmt;
use std::collections::{DList, Deque};

static HEADER_SIZE: uint = 20;
// For simplicity's sake, let us assume no packet will ever exceed the
// Ethernet maximum transfer unit of 1500 bytes.
static BUF_SIZE: uint = 1500;

macro_rules! u8_to_unsigned_be(
    ($src:ident[$start:expr..$end:expr] -> $t:ty) => ({
        let mut result: $t = 0;
        for i in range(0u, $end-$start+1).rev() {
            result = result | $src[$start+i] as $t << i*8;
        }
        result
    })
)

/// Return current time in microseconds since the UNIX epoch.
fn now_microseconds() -> u32 {
    let t = time::get_time();
    (t.sec * 1_000_000) as u32 + (t.nsec/1000) as u32
}

/// Lazy iterator over bits of a vector of bytes, starting with the LSB
/// (least-significat bit) of the first element of the vector.
struct BitIterator { object: Vec<u8>, current_byte: uint, current_bit: uint }

impl BitIterator {
    fn new(obj: Vec<u8>) -> BitIterator {
        BitIterator { object: obj, current_byte: 0, current_bit: 0 }
    }
}

impl Iterator<u8> for BitIterator {
    fn next(&mut self) -> Option<u8> {
        let result = self.object[self.current_byte] >> self.current_bit & 0x1;

        if self.current_bit + 1 == std::u8::BITS {
            self.current_byte += 1;
        }
        self.current_bit = (self.current_bit + 1) % std::u8::BITS;

        if self.current_byte == self.object.len() {
            return None;
        } else {
            return Some(result);
        }
    }
}

#[allow(dead_code,non_camel_case_types)]
#[deriving(PartialEq,Eq,Show)]
enum UtpPacketType {
    ST_DATA  = 0,
    ST_FIN   = 1,
    ST_STATE = 2,
    ST_RESET = 3,
    ST_SYN   = 4,
}

#[deriving(PartialEq,Eq,Show,Clone)]
enum UtpExtensionType {
    SelectiveAckExtension = 1,
}

#[deriving(Clone)]
struct UtpExtension {
    ty: UtpExtensionType,
    data: Vec<u8>,
}

impl UtpExtension {
    fn len(&self) -> uint {
        1 + self.data.len()
    }

    fn to_bytes(&self) -> Vec<u8> {
        (vec!(self.data.len() as u8)).append(self.data.as_slice())
    }
}

#[allow(dead_code)]
#[deriving(Clone)]
#[packed]
struct UtpPacketHeader {
    type_ver: u8, // type: u4, ver: u4
    extension: u8,
    connection_id: u16,
    timestamp_microseconds: u32,
    timestamp_difference_microseconds: u32,
    wnd_size: u32,
    seq_nr: u16,
    ack_nr: u16,
}

impl UtpPacketHeader {
    /// Set type of packet to the specified type.
    fn set_type(&mut self, t: UtpPacketType) {
        let version = 0x0F & self.type_ver;
        self.type_ver = t as u8 << 4 | version;
    }

    fn get_type(&self) -> UtpPacketType {
        let t: UtpPacketType = unsafe { transmute(self.type_ver >> 4) };
        t
    }

    fn get_version(&self) -> u8 {
        self.type_ver & 0x0F
    }

    /// Return packet header as a slice of bytes.
    fn bytes(&self) -> &[u8] {
        let buf: &[u8, ..HEADER_SIZE] = unsafe { transmute(self) };
        return buf.as_slice();
    }

    fn len(&self) -> uint {
        return HEADER_SIZE;
    }

    /// Read byte buffer and return corresponding packet header.
    /// It assumes the fields are in network (big-endian) byte order,
    /// preserving it.
    fn decode(buf: &[u8]) -> UtpPacketHeader {
        UtpPacketHeader {
            type_ver: buf[0],
            extension: buf[1],
            connection_id: u8_to_unsigned_be!(buf[2..3] -> u16),
            timestamp_microseconds: u8_to_unsigned_be!(buf[4..7] -> u32),
            timestamp_difference_microseconds: u8_to_unsigned_be!(buf[8..11] -> u32),
            wnd_size: u8_to_unsigned_be!(buf[12..15] -> u32),
            seq_nr: u8_to_unsigned_be!(buf[16..17] -> u16),
            ack_nr: u8_to_unsigned_be!(buf[18..19] -> u16),
        }
    }
}

impl fmt::Show for UtpPacketHeader {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "(type: {}, version: {}, extension: {}, \
                connection_id: {}, timestamp_microseconds: {}, \
                timestamp_difference_microseconds: {}, wnd_size: {}, \
                seq_nr: {}, ack_nr: {})",
                self.get_type(),
                Int::from_be(self.get_version()),
                Int::from_be(self.extension),
                Int::from_be(self.connection_id),
                Int::from_be(self.timestamp_microseconds),
                Int::from_be(self.timestamp_difference_microseconds),
                Int::from_be(self.wnd_size),
                Int::from_be(self.seq_nr),
                Int::from_be(self.ack_nr),
        )
    }
}

#[allow(dead_code)]
struct UtpPacket {
    header: UtpPacketHeader,
    extensions: Vec<UtpExtension>,
    payload: Vec<u8>,
}

impl UtpPacket {
    /// Construct a new, empty packet.
    fn new() -> UtpPacket {
        UtpPacket {
            header: UtpPacketHeader {
                type_ver: ST_DATA as u8 << 4 | 1,
                extension: 0,
                connection_id: 0,
                timestamp_microseconds: 0,
                timestamp_difference_microseconds: 0,
                wnd_size: 0,
                seq_nr: 0,
                ack_nr: 0,
            },
            extensions: Vec::new(),
            payload: Vec::new(),
        }
    }

    #[inline]
    fn set_type(&mut self, t: UtpPacketType) {
        self.header.set_type(t);
    }

    #[inline]
    fn get_type(&self) -> UtpPacketType {
        self.header.get_type()
    }

    #[inline(always)]
    fn seq_nr(&self) -> u16 {
        Int::from_be(self.header.seq_nr)
    }

    #[inline(always)]
    fn ack_nr(&self) -> u16 {
        Int::from_be(self.header.ack_nr)
    }

    #[inline(always)]
    fn connection_id(&self) -> u16 {
        Int::from_be(self.header.connection_id)
    }

    #[inline]
    fn set_wnd_size(&mut self, new_wnd_size: u32) {
        self.header.wnd_size = new_wnd_size.to_be();
    }

    fn wnd_size(&self) -> u32 {
        Int::from_be(self.header.wnd_size)
    }

    /// Set Selective ACK field in packet header and add appropriate data.
    ///
    /// If None is passed, the SACK extension is disabled and the respective
    /// data is flushed. Otherwise, the SACK extension is enabled and the
    /// vector `v` is taken as the extension's payload.
    ///
    /// The length of the SACK extension is expressed in bytes, which
    /// must be a multiple of 4 and at least 4.
    fn set_sack(&mut self, v: Option<Vec<u8>>) {
        match v {
            None => {
                self.header.extension = 0;
                self.extensions = Vec::new();
            },
            Some(bv) => {
                // The length of the SACK extension is expressed in bytes, which
                // must be a multiple of 4 and at least 4.
                assert!(bv.len() >= 4);
                assert!(bv.len() % 4 == 0);

                let extension = UtpExtension {
                    ty: SelectiveAckExtension,
                    data: bv,
                };
                self.extensions.push(extension);
                self.header.extension |= SelectiveAckExtension as u8;
            }
        }
    }

    /// TODO: return slice
    fn bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(self.len());
        buf.push_all(self.header.bytes());
        for extension in self.extensions.iter() {
            buf.push(0u8); // next extension id
            buf.push_all(extension.to_bytes().as_slice());
        }
        buf.push_all(self.payload.as_slice());
        return buf;
    }

    // Faster, unsafe brother of `bytes`.
    fn to_bytes(&self) -> Vec<u8> {
        use std::ptr;

        let mut buf = Vec::with_capacity(self.len());
        unsafe {
            buf.set_len(self.len());

            let header = self.header.bytes();
            ptr::copy_nonoverlapping_memory(buf.as_mut_ptr(), header.as_ptr(), header.len());

            let mut idx = HEADER_SIZE;
            let mut extensions = self.extensions.iter().peekable();
            for extension in extensions {
                // Set type of the next extension in the linked list
                match extensions.peek() {
                    None => *buf.get_mut(idx) = 0,
                    Some(next) => *buf.get_mut(idx) = next.ty as u8,
                }
                // Write the length and payload of the current extension
                let x = buf.as_mut_ptr().offset(idx as int + 1);
                ptr::copy_nonoverlapping_memory(x, extension.to_bytes().as_ptr(), extension.len());
                idx += 1 + extension.len();
            }

            let x = buf.as_mut_ptr().offset(idx as int);
            ptr::copy_nonoverlapping_memory(x, self.payload.as_ptr(), self.payload.len());
        }
        assert!(!buf.is_empty());
        return buf;
    }

    fn len(&self) -> uint {
        let ext_len = self.extensions.iter().fold(0, |acc, ext| acc + ext.len() + 1);
        self.header.len() + self.payload.len() + ext_len
    }

    /// Decode a byte slice and construct the equivalent UtpPacket.
    ///
    /// Note that this method makes no attempt to guess the payload size, saving
    /// all except the initial 20 bytes corresponding to the header as payload.
    /// It's the caller's responsability to use an appropriately sized buffer.
    fn decode(buf: &[u8]) -> UtpPacket {
        let header = UtpPacketHeader::decode(buf);

        let mut extensions = Vec::new();
        let mut idx = HEADER_SIZE;
        let mut kind = header.extension;

        // Consume known extensions and skip over unknown ones
        while idx < buf.len() && kind != 0 {
            let len = buf[idx + 1] as uint;
            let extension_start = idx + 2;
            let payload_start = extension_start + len;

            if kind == SelectiveAckExtension as u8 { // or more generally, a known kind
                let extension = UtpExtension {
                    ty: SelectiveAckExtension,
                    data: Vec::from_slice(buf.slice(extension_start, payload_start)),
                };
                extensions.push(extension);
            }

            kind = buf[idx];
            idx += payload_start;
        }

        let mut payload;
        if idx < buf.len() {
            let len = buf.len() - idx;
            payload = Vec::with_capacity(len);
            unsafe {
                payload.set_len(len);
                let pload = buf.as_ptr().offset(idx as int);
                std::ptr::copy_nonoverlapping_memory(payload.as_mut_ptr(), pload, len);
            };
        } else {
            payload = Vec::new();
        }

        UtpPacket {
            header: header,
            extensions: extensions,
            payload: payload,
        }
    }

    /// Return a clone of this object without the payload
    fn shallow_clone(&self) -> UtpPacket {
        UtpPacket {
            header: self.header.clone(),
            extensions: self.extensions.clone(),
            payload: Vec::new(),
        }
    }
}

impl Clone for UtpPacket {
    fn clone(&self) -> UtpPacket {
        UtpPacket {
            header:  self.header,
            extensions: self.extensions.clone(),
            payload: self.payload.clone(),
        }
    }
}

impl fmt::Show for UtpPacket {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.header.fmt(f)
    }
}

#[allow(non_camel_case_types)]
#[deriving(PartialEq,Eq,Show)]
enum UtpSocketState {
    CS_NEW,
    CS_CONNECTED,
    CS_SYN_SENT,
    CS_FIN_RECEIVED,
    CS_FIN_SENT,
    CS_RST_RECEIVED,
    CS_CLOSED,
    CS_EOF,
}

/// A uTP (Micro Transport Protocol) socket.
pub struct UtpSocket {
    socket: UdpSocket,
    connected_to: SocketAddr,
    sender_connection_id: u16,
    receiver_connection_id: u16,
    seq_nr: u16,
    ack_nr: u16,
    state: UtpSocketState,

    // Received but not acknowledged packets
    incoming_buffer: Vec<UtpPacket>,
    // Sent but not yet acknowledged packets
    send_window: Vec<UtpPacket>,
    unsent_queue: DList<UtpPacket>,
    duplicate_ack_count: uint,
    last_acked: u16,
    last_acked_timestamp: u32,

    rtt: int,
    rtt_variance: int,
    timeout: int,

    pending_data: Vec<u8>,
    max_window: uint,
    curr_window: uint,
}

impl UtpSocket {
    /// Create a UTP socket from the given address.
    #[unstable]
    pub fn bind(addr: SocketAddr) -> IoResult<UtpSocket> {
        let skt = UdpSocket::bind(addr);
        let connection_id = random::<u16>();
        match skt {
            Ok(x)  => Ok(UtpSocket {
                socket: x,
                connected_to: addr,
                receiver_connection_id: connection_id,
                sender_connection_id: connection_id + 1,
                seq_nr: 1,
                ack_nr: 0,
                state: CS_NEW,
                incoming_buffer: Vec::new(),
                send_window: Vec::new(),
                unsent_queue: DList::new(),
                duplicate_ack_count: 0,
                last_acked: 0,
                last_acked_timestamp: 0,
                rtt: 0,
                rtt_variance: 0,
                timeout: 1000,
                pending_data: Vec::with_capacity(BUF_SIZE),
                max_window: 0,
                curr_window: 0,
            }),
            Err(e) => Err(e)
        }
    }

    /// Open a uTP connection to a remote host by hostname or IP address.
    #[unstable]
    pub fn connect(mut self, other: SocketAddr) -> IoResult<UtpSocket> {
        use std::io::{IoError, ConnectionFailed};

        self.connected_to = other;
        assert_eq!(self.receiver_connection_id + 1, self.sender_connection_id);

        let mut packet = UtpPacket::new();
        packet.set_type(ST_SYN);
        packet.header.connection_id = self.receiver_connection_id.to_be();
        packet.header.seq_nr = self.seq_nr.to_be();

        let mut len = 0;
        let mut addr = self.connected_to;
        let mut buf = [0, ..BUF_SIZE];

        for _ in range(0u, 5) {
            packet.header.timestamp_microseconds = now_microseconds().to_be();

            // Send packet
            try!(self.socket.send_to(packet.to_bytes().as_slice(), other));
            self.state = CS_SYN_SENT;
            debug!("self.state: {}", self.state);

            // Validate response
            self.socket.set_read_timeout(Some(500));
            match self.socket.recv_from(buf) {
                Ok((read, src)) => {
                    len = read;
                    addr = src;
                    break;
                },
                Err(ref e) if e.kind == std::io::TimedOut => continue,
                Err(e) => fail!("{}", e),
            };

        }
        assert!(len == HEADER_SIZE);
        assert!(addr == self.connected_to);

        let packet = UtpPacket::decode(buf.slice_to(len));
        if packet.get_type() != ST_STATE {
            return Err(IoError {
                kind: ConnectionFailed,
                desc: "The remote peer sent an invalid reply",
                detail: None,
            });
        }

        self.ack_nr = packet.seq_nr();
        self.state = CS_CONNECTED;
        self.seq_nr += 1;

        debug!("connected to: {}", self.connected_to);

        return Ok(self);
    }

    /// Gracefully close connection to peer.
    ///
    /// This method allows both peers to receive all packets still in
    /// flight.
    #[unstable]
    pub fn close(&mut self) -> IoResult<()> {
        // Wait for acknowledgment on pending sent packets
        let mut buf = [0u8, ..BUF_SIZE];
        while !self.send_window.is_empty() {
            match self.recv_from(buf) {
                Ok(_) => {},
                Err(e) => fail!("{}", e),
            }
        }

        let mut packet = UtpPacket::new();
        packet.header.connection_id = self.sender_connection_id.to_be();
        packet.header.seq_nr = self.seq_nr.to_be();
        packet.header.ack_nr = self.ack_nr.to_be();
        packet.header.timestamp_microseconds = now_microseconds().to_be();
        packet.set_type(ST_FIN);

        // Send FIN
        try!(self.socket.send_to(packet.bytes().as_slice(), self.connected_to));
        self.state = CS_FIN_SENT;

        // Receive JAKE
        while self.state != CS_CLOSED {
            match self.recv_from(buf) {
                Ok(_) => {},
                Err(ref e) if e.kind == std::io::EndOfFile => self.state = CS_CLOSED,
                Err(e) => fail!("{}", e),
            };
        }

        Ok(())
    }

    /// Receive data from socket.
    ///
    /// On success, returns the number of bytes read and the sender's address.
    /// Returns CS_EOF after receiving a FIN packet when the remaining
    /// inflight packets are consumed. Subsequent calls return CS_CLOSED.
    #[unstable]
    pub fn recv_from(&mut self, buf: &mut[u8]) -> IoResult<(uint,SocketAddr)> {
        use std::io::{IoError, EndOfFile, Closed};

        if self.state == CS_EOF {
            self.state = CS_CLOSED;
            return Err(IoError {
                kind: EndOfFile,
                desc: "End of file reached",
                detail: None,
            });
        }

        if self.state == CS_CLOSED {
            return Err(IoError {
                kind: Closed,
                desc: "Connection closed",
                detail: None,
            });
        }

        match self.flush_incoming_buffer(buf, 0) {
            0 => self.recv(buf),
            read => Ok((read, self.connected_to)),
        }
    }

    fn recv(&mut self, buf: &mut[u8]) -> IoResult<(uint,SocketAddr)> {
        use std::io::{IoError, TimedOut, ConnectionReset};

        let mut b = [0, ..BUF_SIZE + HEADER_SIZE];
        if self.state != CS_NEW {
            debug!("setting read timeout of {} ms", self.timeout);
            self.socket.set_read_timeout(Some(self.timeout as u64));
        }
        let (read, src) = match self.socket.recv_from(b) {
            Err(ref e) if e.kind == TimedOut => {
                debug!("recv_from timed out");
                self.timeout = self.timeout * 2;
                self.send_fast_resend_request();
                return Ok((0, self.connected_to));
            },
            Ok(x) => x,
            Err(e) => return Err(e),
        };
        let packet = UtpPacket::decode(b.slice_to(read));
        debug!("received {}", packet.header);

        if packet.get_type() == ST_RESET {
            return Err(IoError {
                kind: ConnectionReset,
                desc: "Remote host aborted connection (incorrect connection id)",
                detail: None,
            });
        }

        // TODO: move this to handle_packet?
        if packet.get_type() == ST_SYN {
            self.connected_to = src;
        }

        let shallow_clone = packet.shallow_clone();

        if packet.get_type() == ST_DATA && self.ack_nr < packet.seq_nr() {
            self.insert_into_buffer(packet);
        }

        match self.handle_packet(shallow_clone) {
            Some(pkt) => {
                let mut pkt = pkt;
                pkt.set_wnd_size(BUF_SIZE as u32);
                try!(self.socket.send_to(pkt.bytes().as_slice(), src));
                debug!("sent {}", pkt.header);
            },
            None => {}
        };

        // Flush incoming buffer if possible
        let read = self.flush_incoming_buffer(buf, 0);

        Ok((read, src))
    }

    #[allow(missing_doc)]
    #[deprecated = "renamed to `recv_from`"]
    pub fn recvfrom(&mut self, buf: &mut[u8]) -> IoResult<(uint,SocketAddr)> {
        self.recv_from(buf)
    }

    fn prepare_reply(&self, original: &UtpPacketHeader, t: UtpPacketType) -> UtpPacket {
        let mut resp = UtpPacket::new();
        resp.set_type(t);
        let self_t_micro: u32 = now_microseconds();
        let other_t_micro: u32 = Int::from_be(original.timestamp_microseconds);
        resp.header.timestamp_microseconds = self_t_micro.to_be();
        resp.header.timestamp_difference_microseconds = (self_t_micro - other_t_micro).to_be();
        resp.header.connection_id = self.sender_connection_id.to_be();
        resp.header.seq_nr = self.seq_nr.to_be();
        resp.header.ack_nr = self.ack_nr.to_be();

        resp
    }

    /// Discards sequential, ordered packets in incoming buffer, starting from
    /// the most recently acknowledged to the most recent, as long as there are
    /// no missing packets. The discarded packets' payload is written to the
    /// slice `buf`, starting in position `start`.
    /// Returns the last written index.
    fn flush_incoming_buffer(&mut self, buf: &mut [u8], start: uint) -> uint {
        let mut idx = start;

        if !self.pending_data.is_empty() {
            let len = std::cmp::min(buf.len() - idx, self.pending_data.len());
            unsafe {
                let dest = buf.as_mut_ptr().offset(idx as int);
                std::ptr::copy_nonoverlapping_memory(dest, self.pending_data.as_ptr(), len);
            }

            if len == self.pending_data.len() {
                unsafe { self.pending_data.set_len(0); }
                let packet = self.incoming_buffer.remove(0).unwrap();
                debug!("Removing packet from buffer: {}", packet);
                self.ack_nr = packet.seq_nr();
                return idx + len;
            } else {
                self.pending_data = Vec::from_slice(self.pending_data.slice_from(len));
            }
        }

        while !self.incoming_buffer.is_empty() &&
            (self.ack_nr == self.incoming_buffer[0].seq_nr() ||
             self.ack_nr + 1 == self.incoming_buffer[0].seq_nr())
        {
            // Copy as much as possible to the buffer
            let len = std::cmp::min(self.incoming_buffer[0].payload.len(), buf.len() - idx);
            unsafe {
                let dst = buf.as_mut_ptr().offset(idx as int);
                let src = self.incoming_buffer[0].payload.as_ptr();
                std::ptr::copy_nonoverlapping_memory(dst, src, len);
            }
            idx += len;

            if self.incoming_buffer[0].payload.len() <= buf.len() - idx {
                let packet = self.incoming_buffer.remove(0).unwrap();
                debug!("Removing packet from buffer: {}", packet);
                self.ack_nr = packet.seq_nr();
            } else {
                // Copy the remaining data to pending_data
                unsafe {
                    let src = self.incoming_buffer[0].payload.as_ptr().offset(len as int);
                    let dst = self.pending_data.as_mut_ptr();
                    let len = self.incoming_buffer[0].payload.len() - len;
                    if len > self.pending_data.capacity() {
                        self.pending_data.reserve(len);
                    }
                    std::ptr::copy_nonoverlapping_memory(dst, src, len);
                    self.pending_data.set_len(len);
                }
            }

            if buf.len() == idx {
                return idx;
            }
        }

        return idx;
    }

    /// Send data on socket to the given address. Returns nothing on success.
    //
    // # Implementation details
    //
    // This method inserts packets into the send buffer and keeps trying to
    // advance the send window until an ACK corresponding to the last packet is
    // received.
    //
    // Note that the buffer passed to `send_to` might exceed the maximum packet
    // size, which will result in the data being split over several packets.
    #[unstable]
    pub fn send_to(&mut self, buf: &[u8], dst: SocketAddr) -> IoResult<()> {
        use std::io::{IoError, Closed};

        if self.state == CS_CLOSED {
            return Err(IoError {
                kind: Closed,
                desc: "Connection closed",
                detail: None,
            });
        }

        for chunk in buf.chunks(BUF_SIZE) {
            let mut packet = UtpPacket::new();
            packet.set_type(ST_DATA);
            packet.header.timestamp_microseconds = now_microseconds().to_be();
            packet.header.seq_nr = self.seq_nr.to_be();
            packet.header.ack_nr = self.ack_nr.to_be();
            packet.header.connection_id = self.sender_connection_id.to_be();
            packet.payload = Vec::with_capacity(chunk.len());
            unsafe {
                std::ptr::copy_nonoverlapping_memory(packet.payload.as_mut_ptr(), chunk.as_ptr(), chunk.len());
                packet.payload.set_len(chunk.len());
            }

            self.unsent_queue.push(packet);
            self.seq_nr += 1;
        }

        // Flush unsent packet queue
        self.send();

        // Consume acknowledgements until latest packet
        let mut buf = [0, ..BUF_SIZE];
        while self.last_acked < self.seq_nr - 1 {
            try!(self.recv_from(buf));
        }

        Ok(())
    }

    /// Send every packet in the unsent packet queue.
    fn send(&mut self) {
        let dst = self.connected_to;
        loop {
            while self.curr_window > self.max_window {
                let mut buf = [0, ..BUF_SIZE];
                self.recv_from(buf);
            }

            let packet = match self.unsent_queue.pop_front() {
                None => break,
                Some(packet) => packet,
            };

            match self.socket.send_to(packet.to_bytes().as_slice(), dst) {
                Ok(_) => {},
                Err(ref e) => fail!("{}", e),
            }
            debug!("sent {}", packet);
            self.curr_window += packet.len();
            self.send_window.push(packet);
        }
    }


    #[allow(missing_doc)]
    #[deprecated = "renamed to `send_to`"]
    pub fn sendto(&mut self, buf: &[u8], dst: SocketAddr) -> IoResult<()> {
        self.send_to(buf, dst)
    }

    /// Send fast resend request.
    ///
    /// Sends three identical ACK/STATE packets to the remote host, signalling a
    /// fast resend request.
    fn send_fast_resend_request(&mut self) {
        let mut packet = UtpPacket::new();
        packet.set_wnd_size(BUF_SIZE as u32);
        packet.set_type(ST_STATE);
        packet.header.ack_nr = self.ack_nr.to_be();
        packet.header.seq_nr = self.seq_nr.to_be();
        packet.header.connection_id = self.sender_connection_id.to_be();

        for _ in range(0u, 3) {
            let t = now_microseconds();
            packet.header.timestamp_microseconds = t.to_be();
            packet.header.timestamp_difference_microseconds = (t - self.last_acked_timestamp).to_be();
            match self.socket.send_to(packet.bytes().as_slice(), self.connected_to) {
                Ok(_) => {},
                Err(e) => fail!("{}", e),
            }
            debug!("sent {}", packet.header);
        }
    }

    /// Handle incoming packet, updating socket state accordingly.
    ///
    /// Returns appropriate reply packet, if needed.
    fn handle_packet(&mut self, packet: UtpPacket) -> Option<UtpPacket> {
        // Reset connection if connection id doesn't match and this isn't a SYN
        if packet.get_type() != ST_SYN &&
           !(packet.connection_id() == self.sender_connection_id ||
           packet.connection_id() == self.receiver_connection_id) {
            return Some(self.prepare_reply(&packet.header, ST_RESET));
        }

        // Acknowledge only if the packet strictly follows the previous one
        if self.ack_nr + 1 == packet.seq_nr() {
            self.ack_nr = packet.seq_nr();
        }

        self.max_window = Int::from_be(packet.header.wnd_size) as uint;
        debug!("self.max_window: {}", self.max_window);

        match packet.header.get_type() {
            ST_SYN => { // Respond with an ACK and populate own fields
                // Update socket information for new connections
                self.ack_nr = packet.seq_nr();
                self.seq_nr = random();
                self.receiver_connection_id = packet.connection_id() + 1;
                self.sender_connection_id = packet.connection_id();
                self.state = CS_CONNECTED;

                Some(self.prepare_reply(&packet.header, ST_STATE))
            }
            ST_DATA => {
                let mut reply = self.prepare_reply(&packet.header, ST_STATE);

                if self.ack_nr + 1 < packet.seq_nr() {
                    debug!("current ack_nr ({}) is behind received packet seq_nr ({})",
                           self.ack_nr, packet.seq_nr());

                    // Set SACK extension payload if the packet is not in order
                    let mut stashed = self.incoming_buffer.iter()
                        .map(|pkt| pkt.seq_nr())
                        .filter(|&seq_nr| seq_nr > self.ack_nr);

                    let mut sack = Vec::new();
                    for seq_nr in stashed {
                        let diff = seq_nr - self.ack_nr - 2;
                        let byte = (diff / 8) as uint;
                        let bit = (diff % 8) as uint;

                        if byte >= sack.len() {
                            sack.push(0u8);
                        }

                        let mut bitarray = sack.pop().unwrap();
                        bitarray |= 1 << bit;
                        sack.push(bitarray);
                    }

                    // Make sure the amount of elements in the SACK vector is a
                    // multiple of 4
                    if sack.len() % 4 != 0 {
                        let len = sack.len();
                        sack.grow((len / 4 + 1) * 4 - len, &0);
                    }

                    if sack.len() > 0 {
                        reply.set_sack(Some(sack));
                    }
                }

                Some(reply)
            },
            ST_FIN => {
                self.state = CS_FIN_RECEIVED;

                // If all packets are received and handled
                if self.pending_data.is_empty() &&
                    self.incoming_buffer.is_empty() &&
                    self.ack_nr == packet.seq_nr()
                {
                    self.state = CS_EOF;
                    Some(self.prepare_reply(&packet.header, ST_STATE))
                } else {
                    debug!("FIN received but there are missing packets");
                    None
                }
            }
            ST_STATE => {
                let packet_rtt = Int::from_be(packet.header.timestamp_difference_microseconds) as int;
                let delta = self.rtt - packet_rtt;
                self.rtt_variance += (std::num::abs(delta) - self.rtt_variance) / 4;
                self.rtt += (packet_rtt - self.rtt) / 8;
                self.timeout = std::cmp::max(self.rtt + self.rtt_variance * 4, 500);

                debug!("packet_rtt: {}", packet_rtt);
                debug!("delta: {}", delta);
                debug!("self.rtt_variance: {}", self.rtt_variance);
                debug!("self.rtt: {}", self.rtt);
                debug!("self.timeout: {}", self.timeout);

                if packet.ack_nr() == self.last_acked {
                    self.duplicate_ack_count += 1;
                } else {
                    self.last_acked = packet.ack_nr();
                    self.last_acked_timestamp = now_microseconds();
                    self.duplicate_ack_count = 1;
                }

                // Process extensions, if any
                for extension in packet.extensions.iter() {
                    if extension.ty == SelectiveAckExtension {
                        let bits = BitIterator::new(extension.data.clone());
                        // If three or more packets are acknowledged past the implicit missing one,
                        // assume it was lost.
                        if bits.filter(|&bit| bit == 1).count() >= 3 {
                            let packet = self.send_window.iter().find(|pkt| pkt.seq_nr() == packet.ack_nr() + 1).unwrap();
                            debug!("sending {}", packet);
                            self.socket.send_to(packet.bytes().as_slice(), self.connected_to);
                        }

                        let bits = BitIterator::new(extension.data.clone());
                        for (idx, received) in bits.map(|bit| bit == 1).enumerate() {
                            let seq_nr = packet.ack_nr() + 2 + idx as u16;
                            if received {
                                debug!("SACK: packet {} received", seq_nr);
                            } else if seq_nr < self.seq_nr {
                                debug!("SACK: packet {} lost", seq_nr);
                                match self.send_window.iter().find(|pkt| pkt.seq_nr() == seq_nr) {
                                    None => debug!("Packet {} not found", seq_nr),
                                    Some(packet) => {
                                        match self.socket.send_to(packet.bytes().as_slice(), self.connected_to) {
                                            Ok(_) => {},
                                            Err(e) => fail!("{}", e),
                                        }
                                        debug!("sent {}", packet);
                                    }
                                }
                            } else {
                                break;
                            }
                        }
                    } else {
                        debug!("Unknown extension {}, ignoring", extension.ty);
                    }
                }

                // Three duplicate ACKs, must resend packets since `ack_nr + 1`
                // TODO: checking if the send buffer isn't empty isn't a
                // foolproof way to differentiate between triple-ACK and three
                // keep alives spread in time
                if !self.send_window.is_empty() && self.duplicate_ack_count == 3 {
                    for packet in self.send_window.iter().take_while(|pkt| pkt.seq_nr() <= packet.ack_nr() + 1) {
                        debug!("resending: {}", packet);
                        match self.socket.send_to(packet.bytes().as_slice(), self.connected_to) {
                            Ok(_) => {},
                            Err(e) => fail!("{}", e),
                        }
                    }
                }

                // Success, advance send window
                while !self.send_window.is_empty() &&
                    self.send_window[0].seq_nr() <= self.last_acked {
                    let packet = self.send_window.remove(0).unwrap();
                    self.curr_window -= packet.len();
                }
                debug!("self.curr_window: {}", self.curr_window);

                if self.state == CS_FIN_SENT &&
                    packet.ack_nr() == self.seq_nr {
                    self.state = CS_CLOSED;
                }

                None
            },
            ST_RESET => { // TODO
                self.state = CS_RST_RECEIVED;
                None
            },
        }
    }

    /// Insert a packet into the socket's buffer.
    ///
    /// The packet is inserted in such a way that the buffer is
    /// ordered ascendingly by their sequence number. This allows
    /// storing packets that were received out of order.
    ///
    /// Inserting a duplicate of a packet will replace the one in the buffer if
    /// it's more recent (larger timestamp).
    fn insert_into_buffer(&mut self, packet: UtpPacket) {
        let mut i = 0;
        for pkt in self.incoming_buffer.iter() {
            if pkt.seq_nr() >= packet.seq_nr() {
                break;
            }
            i += 1;
        }

        if !self.incoming_buffer.is_empty() && i < self.incoming_buffer.len() &&
            self.incoming_buffer[i].header.seq_nr == packet.header.seq_nr {
            self.incoming_buffer.remove(i);
        }
    self.incoming_buffer.insert(i, packet);
    }
}

impl Clone for UtpSocket {
    fn clone(&self) -> UtpSocket {
        UtpSocket {
            socket: self.socket.clone(),
            connected_to: self.connected_to,
            receiver_connection_id: self.receiver_connection_id,
            sender_connection_id: self.sender_connection_id,
            seq_nr: self.seq_nr,
            ack_nr: self.ack_nr,
            state: self.state,
            incoming_buffer: Vec::new(),
            send_window: Vec::new(),
            unsent_queue: DList::new(),
            duplicate_ack_count: 0,
            last_acked: 0,
            last_acked_timestamp: 0,
            rtt: 0,
            rtt_variance: 0,
            timeout: 500,
            pending_data: Vec::with_capacity(BUF_SIZE),
            max_window: 0,
            curr_window: 0,
        }
    }
}

/// Stream interface for UtpSocket.
pub struct UtpStream {
    socket: UtpSocket,
}

impl UtpStream {
    /// Create a uTP stream listening on the given address.
    #[unstable]
    pub fn bind(addr: SocketAddr) -> IoResult<UtpStream> {
        let socket = UtpSocket::bind(addr);
        match socket {
            Ok(s)  => Ok(UtpStream { socket: s }),
            Err(e) => Err(e),
        }
    }

    /// Open a uTP connection to a remote host by hostname or IP address.
    #[unstable]
    pub fn connect(dst: SocketAddr) -> IoResult<UtpStream> {
        use std::io::net::ip::Ipv4Addr;

        // Port 0 means the operating system gets to choose it
        let my_addr = SocketAddr { ip: Ipv4Addr(127,0,0,1), port: 0 };
        let socket = match UtpSocket::bind(my_addr) {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

        match socket.connect(dst) {
            Ok(socket) => Ok(UtpStream { socket: socket }),
            Err(e) => Err(e),
        }
    }

    /// Gracefully close connection to peer.
    ///
    /// This method allows both peers to receive all packets still in
    /// flight.
    #[unstable]
    pub fn close(&mut self) -> IoResult<()> {
        self.socket.close()
    }
}

impl Reader for UtpStream {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<uint> {
        match self.socket.recv_from(buf) {
            Ok((read, _src)) => Ok(read),
            Err(e) => Err(e),
        }
    }
}

impl Writer for UtpStream {
    fn write(&mut self, buf: &[u8]) -> IoResult<()> {
        let dst = self.socket.connected_to;
        self.socket.send_to(buf, dst)
    }
}

#[cfg(test)]
mod test {
    use super::{UtpSocket, UtpPacket, UtpStream};
    use super::{ST_STATE, ST_FIN, ST_DATA, ST_RESET, ST_SYN};
    use super::{BUF_SIZE, HEADER_SIZE};
    use super::{CS_CONNECTED, CS_NEW, CS_CLOSED, CS_EOF};
    use std::rand::random;
    use std::io::test::next_test_ip4;

    macro_rules! expect_eq(
        ($left:expr, $right:expr) => (
            if !($left == $right) {
                fail!("expected {}, got {}", $right, $left);
            }
        );
    )

    macro_rules! iotry(
        ($e:expr) => (match $e { Ok(e) => e, Err(e) => fail!("{}", e) })
    )

    #[test]
    fn test_packet_decode() {
        let buf = [0x21, 0x00, 0x41, 0xa8, 0x99, 0x2f, 0xd0, 0x2a, 0x9f, 0x4a,
                   0x26, 0x21, 0x00, 0x10, 0x00, 0x00, 0x3a, 0xf2, 0x6c, 0x79];
        let pkt = UtpPacket::decode(buf);
        assert_eq!(pkt.header.get_version(), 1);
        assert_eq!(pkt.header.get_type(), ST_STATE);
        assert_eq!(pkt.header.extension, 0);
        assert_eq!(pkt.connection_id(), 16808);
        assert_eq!(Int::from_be(pkt.header.timestamp_microseconds), 2570047530);
        assert_eq!(Int::from_be(pkt.header.timestamp_difference_microseconds), 2672436769);
        assert_eq!(Int::from_be(pkt.header.wnd_size), ::std::num::pow(2u32, 20));
        assert_eq!(pkt.seq_nr(), 15090);
        assert_eq!(pkt.ack_nr(), 27769);
        assert_eq!(pkt.len(), buf.len());
        assert!(pkt.payload.is_empty());
    }

    #[test]
    fn test_decode_packet_with_extension() {
        use super::SelectiveAckExtension;

        let buf = [0x21, 0x01, 0x41, 0xa7, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                   0x00, 0x00, 0x00, 0x00, 0x05, 0xdc, 0xab, 0x53, 0x3a, 0xf5,
                   0x00, 0x04, 0x00, 0x00, 0x00, 0x00];
        let packet = UtpPacket::decode(buf);
        assert_eq!(packet.header.get_version(), 1);
        assert_eq!(packet.header.get_type(), ST_STATE);
        assert_eq!(packet.header.extension, 1);
        assert_eq!(packet.connection_id(), 16807);
        assert_eq!(Int::from_be(packet.header.timestamp_microseconds), 0);
        assert_eq!(Int::from_be(packet.header.timestamp_difference_microseconds), 0);
        assert_eq!(Int::from_be(packet.header.wnd_size), 1500);
        assert_eq!(packet.seq_nr(), 43859);
        assert_eq!(packet.ack_nr(), 15093);
        assert_eq!(packet.len(), buf.len());
        assert!(packet.payload.is_empty());
        assert!(packet.extensions.len() == 1);
        assert!(packet.extensions[0].ty == SelectiveAckExtension);
        assert!(packet.extensions[0].data == vec!(0,0,0,0));
        assert!(packet.extensions[0].len() == 1 + packet.extensions[0].data.len());
        assert!(packet.extensions[0].len() == 5);
    }

    #[test]
    fn test_decode_packet_with_unknown_extensions() {
        use super::SelectiveAckExtension;

        let buf = [0x21, 0x01, 0x41, 0xa7, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                   0x00, 0x00, 0x00, 0x00, 0x05, 0xdc, 0xab, 0x53, 0x3a, 0xf5,
                   0xff, 0x04, 0x00, 0x00, 0x00, 0x00, // Imaginary extension
                   0x00, 0x04, 0x00, 0x00, 0x00, 0x00];
        let packet = UtpPacket::decode(buf);
        assert_eq!(packet.header.get_version(), 1);
        assert_eq!(packet.header.get_type(), ST_STATE);
        assert_eq!(packet.header.extension, 1);
        assert_eq!(packet.connection_id(), 16807);
        assert_eq!(Int::from_be(packet.header.timestamp_microseconds), 0);
        assert_eq!(Int::from_be(packet.header.timestamp_difference_microseconds), 0);
        assert_eq!(Int::from_be(packet.header.wnd_size), 1500);
        assert_eq!(packet.seq_nr(), 43859);
        assert_eq!(packet.ack_nr(), 15093);
        assert!(packet.payload.is_empty());
        assert!(packet.extensions.len() == 1);
        assert!(packet.extensions[0].ty == SelectiveAckExtension);
        assert!(packet.extensions[0].data == vec!(0,0,0,0));
        assert!(packet.extensions[0].len() == 1 + packet.extensions[0].data.len());
        assert!(packet.extensions[0].len() == 5);
    }

    #[test]
    fn test_packet_encode() {
        let payload = Vec::from_slice("Hello\n".as_bytes());
        let (timestamp, timestamp_diff): (u32, u32) = (15270793, 1707040186);
        let (connection_id, seq_nr, ack_nr): (u16, u16, u16) = (16808, 15090, 17096);
        let window_size: u32 = 1048576;
        let mut pkt = UtpPacket::new();
        pkt.set_type(ST_DATA);
        pkt.header.timestamp_microseconds = timestamp.to_be();
        pkt.header.timestamp_difference_microseconds = timestamp_diff.to_be();
        pkt.header.connection_id = connection_id.to_be();
        pkt.header.seq_nr = seq_nr.to_be();
        pkt.header.ack_nr = ack_nr.to_be();
        pkt.header.wnd_size = window_size.to_be();
        pkt.payload = payload.clone();
        let header = pkt.header;
        let buf: &[u8] = [0x01, 0x00, 0x41, 0xa8, 0x00, 0xe9, 0x03, 0x89,
                    0x65, 0xbf, 0x5d, 0xba, 0x00, 0x10, 0x00, 0x00,
                    0x3a, 0xf2, 0x42, 0xc8, 0x48, 0x65, 0x6c, 0x6c,
                    0x6f, 0x0a];

        assert_eq!(pkt.len(), buf.len());
        assert_eq!(pkt.len(), HEADER_SIZE + payload.len());
        assert_eq!(pkt.payload, payload);
        assert_eq!(header.get_version(), 1);
        assert_eq!(header.get_type(), ST_DATA);
        assert_eq!(header.extension, 0);
        assert_eq!(Int::from_be(header.connection_id), connection_id);
        assert_eq!(Int::from_be(header.seq_nr), seq_nr);
        assert_eq!(Int::from_be(header.ack_nr), ack_nr);
        assert_eq!(Int::from_be(header.wnd_size), window_size);
        assert_eq!(Int::from_be(header.timestamp_microseconds), timestamp);
        assert_eq!(Int::from_be(header.timestamp_difference_microseconds), timestamp_diff);
        assert_eq!(pkt.bytes(), Vec::from_slice(buf));
    }

    #[test]
    fn test_reversible() {
        let buf: &[u8] = [0x01, 0x00, 0x41, 0xa8, 0x00, 0xe9, 0x03, 0x89,
                    0x65, 0xbf, 0x5d, 0xba, 0x00, 0x10, 0x00, 0x00,
                    0x3a, 0xf2, 0x42, 0xc8, 0x48, 0x65, 0x6c, 0x6c,
                    0x6f, 0x0a];
        assert_eq!(UtpPacket::decode(buf).bytes().as_slice(), buf);
    }

    #[test]
    fn test_socket_ipv4() {
        let (server_addr, client_addr) = (next_test_ip4(), next_test_ip4());

        let client = iotry!(UtpSocket::bind(client_addr));
        let mut server = iotry!(UtpSocket::bind(server_addr));

        assert!(server.state == CS_NEW);
        assert!(client.state == CS_NEW);

        // Check proper difference in client's send connection id and receive connection id
        assert_eq!(client.sender_connection_id, client.receiver_connection_id + 1);

        spawn(proc() {
            let client = iotry!(client.connect(server_addr));
            assert!(client.state == CS_CONNECTED);
            assert_eq!(client.connected_to, server_addr);
            drop(client);
        });

        let mut buf = [0u8, ..BUF_SIZE];
        match server.recv_from(buf) {
            e => println!("{}", e),
        }
        // After establishing a new connection, the server's ids are a mirror of the client's.
        assert_eq!(server.receiver_connection_id, server.sender_connection_id + 1);
        assert_eq!(server.connected_to, client_addr);

        assert!(server.state == CS_CONNECTED);
        drop(server);
    }

    #[test]
    fn test_recvfrom_on_closed_socket() {
        use std::io::{Closed, EndOfFile};

        let (server_addr, client_addr) = (next_test_ip4(), next_test_ip4());

        let client = iotry!(UtpSocket::bind(client_addr));
        let mut server = iotry!(UtpSocket::bind(server_addr));

        assert!(server.state == CS_NEW);
        assert!(client.state == CS_NEW);

        spawn(proc() {
            let mut client = iotry!(client.connect(server_addr));
            assert!(client.state == CS_CONNECTED);
            assert_eq!(client.close(), Ok(()));
            drop(client);
        });

        // Make the server listen for incoming connections
        let mut buf = [0u8, ..BUF_SIZE];
        let _resp = server.recv_from(buf);
        assert!(server.state == CS_CONNECTED);

        // Closing the connection is fine
        match server.recv_from(buf) {
            Err(e) => fail!("{}", e),
            _ => {},
        }
        expect_eq!(server.state, CS_EOF);

        // Trying to listen on the socket after closing it raises an
        // EOF error
        match server.recv_from(buf) {
            Err(e) => expect_eq!(e.kind, EndOfFile),
            v => fail!("expected {}, got {}", EndOfFile, v),
        }

        expect_eq!(server.state, CS_CLOSED);

        // Trying again raises a Closed error
        match server.recv_from(buf) {
            Err(e) => expect_eq!(e.kind, Closed),
            v => fail!("expected {}, got {}", Closed, v),
        }

        drop(server);
    }

    #[test]
    fn test_sendto_on_closed_socket() {
        use std::io::Closed;

        let (server_addr, client_addr) = (next_test_ip4(), next_test_ip4());

        let client = iotry!(UtpSocket::bind(client_addr));
        let mut server = iotry!(UtpSocket::bind(server_addr));

        assert!(server.state == CS_NEW);
        assert!(client.state == CS_NEW);

        spawn(proc() {
            let client = iotry!(client.connect(server_addr));
            assert!(client.state == CS_CONNECTED);
            let mut buf = [0u8, ..BUF_SIZE];
            let mut client = client;
            iotry!(client.recv_from(buf));
        });

        // Make the server listen for incoming connections
        let mut buf = [0u8, ..BUF_SIZE];
        let (_read, _src) = iotry!(server.recv_from(buf));
        assert!(server.state == CS_CONNECTED);

        iotry!(server.close());
        expect_eq!(server.state, CS_CLOSED);

        // Trying to send to the socket after closing it raises an
        // error
        match server.send_to(buf, client_addr) {
            Err(e) => expect_eq!(e.kind, Closed),
            v => fail!("expected {}, got {}", Closed, v),
        }

        drop(server);
    }

    #[test]
    fn test_acks_on_socket() {
        let (server_addr, client_addr) = (next_test_ip4(), next_test_ip4());
        let (tx, rx) = channel();

        let client = iotry!(UtpSocket::bind(client_addr));
        let server = iotry!(UtpSocket::bind(server_addr));

        spawn(proc() {
            // Make the server listen for incoming connections
            let mut server = server;
            let mut buf = [0u8, ..BUF_SIZE];
            let _resp = server.recv_from(buf);
            tx.send(server.seq_nr);

            // Close the connection
            iotry!(server.recv_from(buf));

            drop(server);
        });

        let mut client = iotry!(client.connect(server_addr));
        assert!(client.state == CS_CONNECTED);
        let sender_seq_nr = rx.recv();
        let ack_nr = client.ack_nr;
        assert!(ack_nr != 0);
        assert!(ack_nr == sender_seq_nr);
        assert_eq!(client.close(), Ok(()));

        // The reply to both connect (SYN) and close (FIN) should be
        // STATE packets, which don't increase the sequence number
        // and, hence, the receiver's acknowledgement number.
        assert!(client.ack_nr == ack_nr);
        drop(client);
    }

    #[test]
    fn test_handle_packet() {
        //fn test_connection_setup() {
        let initial_connection_id: u16 = random();
        let sender_connection_id = initial_connection_id + 1;
        let server_addr = next_test_ip4();
        let mut socket = iotry!(UtpSocket::bind(server_addr));

        let mut packet = UtpPacket::new();
        packet.set_wnd_size(BUF_SIZE as u32);
        packet.set_type(ST_SYN);
        packet.header.connection_id = initial_connection_id.to_be();
        let sent = packet.header;

        // Do we have a response?
        let response = socket.handle_packet(packet.clone());
        assert!(response.is_some());

        // Is is of the correct type?
        let response = response.unwrap();
        assert!(response.get_type() == ST_STATE);

        // Same connection id on both ends during connection establishment
        assert!(response.header.connection_id == sent.connection_id);

        // Response acknowledges SYN
        assert!(response.header.ack_nr == sent.seq_nr);

        // No payload?
        assert!(response.payload.is_empty());
        //}

        // ---------------------------------

        // fn test_connection_usage() {
        let old_packet = packet;
        let old_response = response;

        let mut packet = UtpPacket::new();
        packet.set_type(ST_DATA);
        packet.header.connection_id = sender_connection_id.to_be();
        packet.header.seq_nr = (old_packet.seq_nr() + 1).to_be();
        packet.header.ack_nr = old_response.header.seq_nr;
        let sent = packet.header;

        let response = socket.handle_packet(packet.clone());
        assert!(response.is_some());

        let response = response.unwrap();
        assert!(response.get_type() == ST_STATE);

        // Sender (i.e., who initated connection and sent SYN) has connection id
        // equal to initial connection id + 1
        // Receiver (i.e., who accepted connection) has connection id equal to
        // initial connection id
        assert!(response.connection_id() == initial_connection_id);
        assert!(response.connection_id() == Int::from_be(sent.connection_id) - 1);

        // Previous packets should be ack'ed
        assert!(response.ack_nr() == Int::from_be(sent.seq_nr));

        // Responses with no payload should not increase the sequence number
        assert!(response.payload.is_empty());
        assert!(response.seq_nr() == old_response.seq_nr());
        // }

        //fn test_connection_teardown() {
        let old_packet = packet;
        let old_response = response;

        let mut packet = UtpPacket::new();
        packet.set_type(ST_FIN);
        packet.header.connection_id = sender_connection_id.to_be();
        packet.header.seq_nr = (old_packet.seq_nr() + 1).to_be();
        packet.header.ack_nr = old_response.header.seq_nr;
        let sent = packet.header;

        let response = socket.handle_packet(packet);
        assert!(response.is_some());

        let response = response.unwrap();

        assert!(response.get_type() == ST_STATE);

        // FIN packets have no payload but the sequence number shouldn't increase
        assert!(Int::from_be(sent.seq_nr) == old_packet.seq_nr() + 1);

        // Nor should the ACK packet's sequence number
        assert!(response.header.seq_nr == old_response.header.seq_nr);

        // FIN should be acknowledged
        assert!(response.header.ack_nr == sent.seq_nr);

        //}
    }

    #[test]
    fn test_response_to_keepalive_ack() {
        // Boilerplate test setup
        let initial_connection_id: u16 = random();
        let server_addr = next_test_ip4();
        let mut socket = iotry!(UtpSocket::bind(server_addr));

        // Establish connection
        let mut packet = UtpPacket::new();
        packet.set_wnd_size(BUF_SIZE as u32);
        packet.set_type(ST_SYN);
        packet.header.connection_id = initial_connection_id.to_be();

        let response = socket.handle_packet(packet.clone());
        assert!(response.is_some());
        let response = response.unwrap();
        assert!(response.get_type() == ST_STATE);

        let old_packet = packet;
        let old_response = response;

        // Now, send a keepalive packet
        let mut packet = UtpPacket::new();
        packet.set_wnd_size(BUF_SIZE as u32);
        packet.set_type(ST_STATE);
        packet.header.connection_id = initial_connection_id.to_be();
        packet.header.seq_nr = (old_packet.seq_nr() + 1).to_be();
        packet.header.ack_nr = old_response.header.seq_nr;

        let response = socket.handle_packet(packet.clone());
        assert!(response.is_none());

        // Send a second keepalive packet, identical to the previous one
        let response = socket.handle_packet(packet.clone());
        assert!(response.is_none());
    }

    #[test]
    fn test_response_to_wrong_connection_id() {
        // Boilerplate test setup
        let initial_connection_id: u16 = random();
        let server_addr = next_test_ip4();
        let mut socket = iotry!(UtpSocket::bind(server_addr));

        // Establish connection
        let mut packet = UtpPacket::new();
        packet.set_wnd_size(BUF_SIZE as u32);
        packet.set_type(ST_SYN);
        packet.header.connection_id = initial_connection_id.to_be();

        let response = socket.handle_packet(packet.clone());
        assert!(response.is_some());
        assert!(response.unwrap().get_type() == ST_STATE);

        // Now, disrupt connection with a packet with an incorrect connection id
        let new_connection_id = initial_connection_id.to_le();

        let mut packet = UtpPacket::new();
        packet.set_wnd_size(BUF_SIZE as u32);
        packet.set_type(ST_STATE);
        packet.header.connection_id = new_connection_id;

        let response = socket.handle_packet(packet.clone());
        assert!(response.is_some());

        let response = response.unwrap();
        assert!(response.get_type() == ST_RESET);
        assert!(response.header.ack_nr == packet.header.seq_nr);
    }

    #[test]
    fn test_utp_stream() {
        let server_addr = next_test_ip4();
        let mut server = iotry!(UtpStream::bind(server_addr));

        spawn(proc() {
            let mut client = iotry!(UtpStream::connect(server_addr));
            iotry!(client.close());
        });

        iotry!(server.read_to_end());
    }

    #[test]
    fn test_utp_stream_small_data() {
        // Fits in a packet
        static len: uint = 1024;
        let data = Vec::from_fn(len, |idx| idx as u8);
        expect_eq!(len, data.len());

        let d = data.clone();
        let server_addr = next_test_ip4();
        let mut server = UtpStream::bind(server_addr);

        spawn(proc() {
            let mut client = iotry!(UtpStream::connect(server_addr));
            iotry!(client.write(d.as_slice()));
            iotry!(client.close());
        });

        let read = iotry!(server.read_to_end());
        assert!(!read.is_empty());
        expect_eq!(read.len(), data.len());
        expect_eq!(read, data);
    }

    #[test]
    fn test_utp_stream_large_data() {
        // Has to be sent over several packets
        static len: uint = 1024 * 1024;
        let data = Vec::from_fn(len, |idx| idx as u8);
        expect_eq!(len, data.len());

        let d = data.clone();
        let server_addr = next_test_ip4();
        let mut server = UtpStream::bind(server_addr);

        spawn(proc() {
            let mut client = iotry!(UtpStream::connect(server_addr));
            iotry!(client.write(d.as_slice()));
            iotry!(client.close());
        });

        let read = iotry!(server.read_to_end());
        assert!(!read.is_empty());
        expect_eq!(read.len(), data.len());
        expect_eq!(read, data);
    }

    #[test]
    fn test_utp_stream_successive_reads() {
        use std::io::Closed;

        static len: uint = 1024;
        let data: Vec<u8> = Vec::from_fn(len, |idx| idx as u8);
        expect_eq!(len, data.len());

        let d = data.clone();
        let server_addr = next_test_ip4();
        let mut server = UtpStream::bind(server_addr);

        spawn(proc() {
            let mut client = iotry!(UtpStream::connect(server_addr));
            iotry!(client.write(d.as_slice()));
            iotry!(client.close());
        });

        iotry!(server.read_to_end());

        let mut buf = [0u8, ..4096];
        match server.read(buf) {
            Err(ref e) if e.kind == Closed => {},
            _ => fail!("should have failed with Closed"),
        };
    }

    #[test]
    fn test_unordered_packets() {
        // Boilerplate test setup
        let initial_connection_id: u16 = random();
        let server_addr = next_test_ip4();
        let mut socket = iotry!(UtpSocket::bind(server_addr));

        // Establish connection
        let mut packet = UtpPacket::new();
        packet.set_wnd_size(BUF_SIZE as u32);
        packet.set_type(ST_SYN);
        packet.header.connection_id = initial_connection_id.to_be();

        let response = socket.handle_packet(packet.clone());
        assert!(response.is_some());
        let response = response.unwrap();
        assert!(response.get_type() == ST_STATE);

        let old_packet = packet;
        let old_response = response;

        let mut window: Vec<UtpPacket> = Vec::new();

        // Now, send a keepalive packet
        let mut packet = UtpPacket::new();
        packet.set_wnd_size(BUF_SIZE as u32);
        packet.set_type(ST_DATA);
        packet.header.connection_id = initial_connection_id.to_be();
        packet.header.seq_nr = (old_packet.seq_nr() + 1).to_be();
        packet.header.ack_nr = old_response.header.seq_nr;
        packet.payload = vec!(1,2,3);
        window.push(packet);

        let mut packet = UtpPacket::new();
        packet.set_wnd_size(BUF_SIZE as u32);
        packet.set_type(ST_DATA);
        packet.header.connection_id = initial_connection_id.to_be();
        packet.header.seq_nr = (old_packet.seq_nr() + 2).to_be();
        packet.header.ack_nr = old_response.header.seq_nr;
        packet.payload = vec!(4,5,6);
        window.push(packet);

        // Send packets in reverse order
        let response = socket.handle_packet(window[1].clone());
        assert!(response.is_some());
        let response = response.unwrap();
        assert!(response.header.ack_nr != window[1].header.seq_nr);

        let response = socket.handle_packet(window[0].clone());
        assert!(response.is_some());
    }

    #[test]
    fn test_socket_unordered_packets() {
        let (server_addr, client_addr) = (next_test_ip4(), next_test_ip4());

        let client = iotry!(UtpSocket::bind(client_addr));
        let mut server = iotry!(UtpSocket::bind(server_addr));

        assert!(server.state == CS_NEW);
        assert!(client.state == CS_NEW);

        // Check proper difference in client's send connection id and receive connection id
        assert_eq!(client.sender_connection_id, client.receiver_connection_id + 1);

        spawn(proc() {
            let mut client = iotry!(client.connect(server_addr));
            assert!(client.state == CS_CONNECTED);
            let mut s = client.socket;
            let mut window: Vec<UtpPacket> = Vec::new();

            for (i, data) in Vec::from_fn(12, |idx| idx as u8 + 1).as_slice().chunks(3).enumerate() {
                let mut packet = UtpPacket::new();
                packet.set_wnd_size(BUF_SIZE as u32);
                packet.set_type(ST_DATA);
                packet.header.connection_id = client.sender_connection_id.to_be();
                packet.header.seq_nr = client.seq_nr.to_be();
                packet.header.ack_nr = client.ack_nr.to_be();
                packet.payload = Vec::from_slice(data);
                window.push(packet.clone());
                client.send_window.push(packet.clone());
                client.seq_nr += 1;
            }

            let mut packet = UtpPacket::new();
            packet.set_wnd_size(BUF_SIZE as u32);
            packet.set_type(ST_FIN);
            packet.header.connection_id = client.sender_connection_id.to_be();
            packet.header.seq_nr = client.seq_nr.to_be();
            packet.header.ack_nr = client.ack_nr.to_be();
            window.push(packet);
            client.seq_nr += 1;

            iotry!(s.send_to(window[3].bytes().as_slice(), server_addr));
            iotry!(s.send_to(window[2].bytes().as_slice(), server_addr));
            iotry!(s.send_to(window[1].bytes().as_slice(), server_addr));
            iotry!(s.send_to(window[0].bytes().as_slice(), server_addr));
            iotry!(s.send_to(window[4].bytes().as_slice(), server_addr));

            for _ in range(0u, 2) {
                let mut buf = [0, ..BUF_SIZE];
                iotry!(s.recv_from(buf));
            }
        });

        let mut buf = [0u8, ..BUF_SIZE];
        match server.recv_from(buf) {
            e => println!("{}", e),
        }
        // After establishing a new connection, the server's ids are a mirror of the client's.
        assert_eq!(server.receiver_connection_id, server.sender_connection_id + 1);

        assert!(server.state == CS_CONNECTED);

        let mut stream = UtpStream { socket: server };
        let expected: Vec<u8> = Vec::from_fn(12, |idx| idx as u8 + 1);

        match stream.read_to_end() {
            Ok(data) => {
                expect_eq!(data.len(), expected.len());
                expect_eq!(data, expected);
            },
            Err(e) => fail!("{}", e),
        }
    }

    #[test]
    fn test_socket_should_not_buffer_syn_packets() {
        use std::io::net::udp::UdpSocket;

        let (server_addr, client_addr) = (next_test_ip4(), next_test_ip4());
        let server = iotry!(UtpSocket::bind(server_addr));
        let client = iotry!(UdpSocket::bind(client_addr));

        let test_syn_raw = [0x41, 0x00, 0x41, 0xa7, 0x00, 0x00, 0x00,
        0x27, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x3a,
        0xf1, 0x00, 0x00];
        let test_syn_pkt = UtpPacket::decode(test_syn_raw);
        let seq_nr = test_syn_pkt.seq_nr();

        spawn(proc() {
            let mut client = client;
            iotry!(client.send_to(test_syn_raw, server_addr));
            client.set_timeout(Some(10));
            let mut buf = [0, ..BUF_SIZE];
            let packet = match client.recv_from(buf) {
                Ok((nread, _src)) => UtpPacket::decode(buf.slice_to(nread)),
                Err(e) => fail!("{}", e),
            };
            expect_eq!(packet.header.ack_nr, seq_nr.to_be());
            drop(client);
        });

        let mut server = server;
        let mut buf = [0, ..20];
        iotry!(server.recv_from(buf));
        assert!(server.ack_nr != 0);
        expect_eq!(server.ack_nr, seq_nr);
        assert!(server.incoming_buffer.is_empty());
    }

    #[test]
    fn test_response_to_triple_ack() {
        let (server_addr, client_addr) = (next_test_ip4(), next_test_ip4());
        let mut server = iotry!(UtpSocket::bind(server_addr));
        let client = iotry!(UtpSocket::bind(client_addr));

        // Fits in a packet
        static len: uint = 1024;
        let data = Vec::from_fn(len, |idx| idx as u8);
        let d = data.clone();
        expect_eq!(len, data.len());

        spawn(proc() {
            let mut client = iotry!(client.connect(server_addr));
            iotry!(client.send_to(d.as_slice(), server_addr));
            iotry!(client.close());
        });

        let mut buf = [0, ..BUF_SIZE];
        // Expect SYN
        iotry!(server.recv_from(buf));

        // Receive data
        let mut data_packet;
        match server.socket.recv_from(buf) {
            Ok((read, _src)) => {
                data_packet = UtpPacket::decode(buf.slice_to(read));
                assert!(data_packet.get_type() == ST_DATA);
                expect_eq!(data_packet.payload, data);
                assert_eq!(data_packet.payload.len(), data.len());
            },
            Err(e) => fail!("{}", e),
        }
        let data_packet = data_packet;

        // Send triple ACK
        let mut packet = UtpPacket::new();
        packet.set_wnd_size(BUF_SIZE as u32);
        packet.set_type(ST_STATE);
        packet.header.seq_nr = server.seq_nr.to_be();
        packet.header.ack_nr = (data_packet.seq_nr() - 1).to_be();
        packet.header.connection_id = server.sender_connection_id.to_be();

        for _ in range(0u, 3) {
            iotry!(server.socket.send_to(packet.bytes().as_slice(), client_addr));
        }

        // Receive data again and check that it's the same we reported as missing
        match server.socket.recv_from(buf) {
            Ok((0, _)) => fail!("Received 0 bytes from socket"),
            Ok((read, _src)) => {
                let packet = UtpPacket::decode(buf.slice_to(read));
                assert_eq!(packet.get_type(), ST_DATA);
                assert_eq!(packet.seq_nr(), data_packet.seq_nr());
                assert!(packet.payload == data_packet.payload);
                let response = server.handle_packet(packet).unwrap();
                iotry!(server.socket.send_to(response.bytes().as_slice(), server.connected_to));
            },
            Err(e) => fail!("{}", e),
        }

        // Receive close
        iotry!(server.recv_from(buf));
    }

    #[test]
    fn test_socket_timeout_request() {
        let (server_addr, client_addr) = (next_test_ip4(), next_test_ip4());

        let client = iotry!(UtpSocket::bind(client_addr));
        let mut server = iotry!(UtpSocket::bind(server_addr));
        let len = 512;
        let data = Vec::from_fn(len, |idx| idx as u8);
        let d = data.clone();

        assert!(server.state == CS_NEW);
        assert!(client.state == CS_NEW);

        // Check proper difference in client's send connection id and receive connection id
        assert_eq!(client.sender_connection_id, client.receiver_connection_id + 1);

        spawn(proc() {
            let mut client = iotry!(client.connect(server_addr));
            assert!(client.state == CS_CONNECTED);
            assert_eq!(client.connected_to, server_addr);
            iotry!(client.send_to(d.as_slice(), server_addr));
            drop(client);
        });

        let mut buf = [0u8, ..BUF_SIZE];
        match server.recv_from(buf) {
            e => println!("{}", e),
        }
        // After establishing a new connection, the server's ids are a mirror of the client's.
        assert_eq!(server.receiver_connection_id, server.sender_connection_id + 1);
        assert_eq!(server.connected_to, client_addr);

        assert!(server.state == CS_CONNECTED);

        // Purposefully read from UDP socket directly and discard it, in order
        // to behave as if the packet was lost and thus trigger the timeout
        // handling in the *next* call to `UtpSocket.recv_from`.
        iotry!(server.socket.recv_from(buf));

        // Set a much smaller than usual timeout, for quicker test completion
        server.timeout = 50;

        // Now wait for the previously discarded packet
        loop {
            match server.recv_from(buf) {
                Ok((0, _)) => continue,
                Ok(_) => break,
                Err(e) => fail!("{}", e),
            }
        }

        drop(server);
    }

    #[test]
    fn test_sorted_buffer_insertion() {
        let server_addr = next_test_ip4();
        let mut socket = iotry!(UtpSocket::bind(server_addr));

        let mut packet = UtpPacket::new();
        packet.header.seq_nr = 1;

        assert!(socket.incoming_buffer.is_empty());

        socket.insert_into_buffer(packet.clone());
        assert_eq!(socket.incoming_buffer.len(), 1);

        packet.header.seq_nr = 2;
        packet.header.timestamp_microseconds = 128;

        socket.insert_into_buffer(packet.clone());
        assert_eq!(socket.incoming_buffer.len(), 2);
        assert_eq!(socket.incoming_buffer[1].header.seq_nr, 2);
        assert_eq!(socket.incoming_buffer[1].header.timestamp_microseconds, 128);

        packet.header.seq_nr = 3;
        packet.header.timestamp_microseconds = 256;

        socket.insert_into_buffer(packet.clone());
        assert_eq!(socket.incoming_buffer.len(), 3);
        assert_eq!(socket.incoming_buffer[2].header.seq_nr, 3);
        assert_eq!(socket.incoming_buffer[2].header.timestamp_microseconds, 256);

        // Replace a packet with a more recent version
        packet.header.seq_nr = 2;
        packet.header.timestamp_microseconds = 456;

        socket.insert_into_buffer(packet.clone());
        assert_eq!(socket.incoming_buffer.len(), 3);
        assert_eq!(socket.incoming_buffer[1].header.seq_nr, 2);
        assert_eq!(socket.incoming_buffer[1].header.timestamp_microseconds, 456);
    }

    #[test]
    fn test_duplicate_packet_handling() {
        let (server_addr, client_addr) = (next_test_ip4(), next_test_ip4());

        let client = iotry!(UtpSocket::bind(client_addr));
        let mut server = iotry!(UtpSocket::bind(server_addr));

        assert!(server.state == CS_NEW);
        assert!(client.state == CS_NEW);

        // Check proper difference in client's send connection id and receive connection id
        assert_eq!(client.sender_connection_id, client.receiver_connection_id + 1);

        spawn(proc() {
            let mut client = iotry!(client.connect(server_addr));
            assert!(client.state == CS_CONNECTED);
            let mut s = client.socket.clone();

            let mut packet = UtpPacket::new();
            packet.set_wnd_size(BUF_SIZE as u32);
            packet.set_type(ST_DATA);
            packet.header.connection_id = client.sender_connection_id.to_be();
            packet.header.seq_nr = client.seq_nr.to_be();
            packet.header.ack_nr = client.ack_nr.to_be();
            packet.payload = vec!(1,2,3);

            // Send two copies of the packet, with different timestamps
            for _ in range(0u, 2) {
                packet.header.timestamp_microseconds = super::now_microseconds();
                iotry!(s.send_to(packet.bytes().as_slice(), server_addr));
            }
            client.seq_nr += 1;

            // Receive one ACK
            for _ in range(0u, 1) {
                let mut buf = [0, ..BUF_SIZE];
                iotry!(s.recv_from(buf));
            }

            iotry!(client.close());
        });

        let mut buf = [0u8, ..BUF_SIZE];
        match server.recv_from(buf) {
            e => println!("{}", e),
        }
        // After establishing a new connection, the server's ids are a mirror of the client's.
        assert_eq!(server.receiver_connection_id, server.sender_connection_id + 1);

        assert!(server.state == CS_CONNECTED);

        let mut stream = UtpStream { socket: server };
        let expected: Vec<u8> = vec!(1,2,3);

        match stream.read_to_end() {
            Ok(data) => {
                println!("{}", data);
                expect_eq!(data.len(), expected.len());
                expect_eq!(data, expected);
            },
            Err(e) => fail!("{}", e),
        }
    }

    #[test]
    fn test_selective_ack_response() {
        let server_addr = next_test_ip4();
        let len = 1024 * 10;
        let data = Vec::from_fn(len, |idx| idx as u8);
        let to_send = data.clone();

        // Client
        spawn(proc() {
            let mut client = iotry!(UtpStream::connect(server_addr));
            client.socket.timeout = 50;

            // Stream.write
            iotry!(client.write(to_send.as_slice()));
            iotry!(client.close());
        });

        // Server
        let mut server = iotry!(UtpSocket::bind(server_addr));

        let mut buf = [0, ..BUF_SIZE];

        // Connect
        iotry!(server.recv_from(buf));

        // Discard packets
        iotry!(server.socket.recv_from(buf));
        iotry!(server.socket.recv_from(buf));
        iotry!(server.socket.recv_from(buf));

        // Generate SACK
        let mut packet = UtpPacket::new();
        packet.header.seq_nr = server.seq_nr.to_be();
        packet.header.ack_nr = (server.ack_nr - 1).to_be();
        packet.header.connection_id = server.sender_connection_id.to_be();
        packet.header.timestamp_microseconds = super::now_microseconds().to_be();
        packet.set_type(ST_STATE);
        packet.set_sack(Some(vec!(12, 0, 0, 0)));

        // Send SACK
        iotry!(server.socket.send_to(packet.bytes().as_slice(), server.connected_to.clone()));

        // Expect to receive "missing" packets
        let mut stream = UtpStream { socket: server };
        let read = iotry!(stream.read_to_end());
        assert!(!read.is_empty());
        expect_eq!(read.len(), data.len());
        expect_eq!(read, data);
    }

    #[test]
    fn test_correct_packet_loss() {
        let (client_addr, server_addr) = (next_test_ip4(), next_test_ip4());

        let mut server = iotry!(UtpStream::bind(server_addr));
        let client = iotry!(UtpSocket::bind(client_addr));
        let len = 1024 * 10;
        let data = Vec::from_fn(len, |idx| idx as u8);
        let to_send = data.clone();

        spawn(proc() {
            let mut client = iotry!(client.connect(server_addr));

            // Send everything except the odd chunks
            let chunks = to_send.as_slice().chunks(BUF_SIZE);
            let dst = client.connected_to;
            for (index, chunk) in chunks.enumerate() {
                let mut packet = UtpPacket::new();
                packet.header.seq_nr = client.seq_nr.to_be();
                packet.header.ack_nr = client.ack_nr.to_be();
                packet.header.connection_id = client.sender_connection_id.to_be();
                packet.header.timestamp_microseconds = super::now_microseconds().to_be();
                packet.payload = Vec::from_slice(chunk);
                packet.set_type(ST_DATA);

                if index % 2 == 0 {
                    iotry!(client.socket.send_to(packet.bytes().as_slice(), dst));
                }

                client.send_window.push(packet);
                client.seq_nr += 1;
            }

            iotry!(client.close());
        });

        let read = iotry!(server.read_to_end());
        assert_eq!(read.len(), data.len());
        assert_eq!(read, data);
    }

    #[test]
    fn test_tolerance_to_small_buffers() {
        use std::io::EndOfFile;

        let server_addr = next_test_ip4();
        let mut server = iotry!(UtpSocket::bind(server_addr));
        let len = 1024;
        let data = Vec::from_fn(len, |idx| idx as u8);
        let to_send = data.clone();

        spawn(proc() {
            let mut client = iotry!(UtpStream::connect(server_addr));
            iotry!(client.write(to_send.as_slice()));
            iotry!(client.close());
        });

        let mut read = Vec::new();
        while server.state != CS_CLOSED {
            let mut small_buffer = [0, ..512];
            match server.recv_from(small_buffer) {
                Ok((0, _src)) => (),
                Ok((len, _src)) => read.push_all(small_buffer.slice_to(len)),
                Err(ref e) if e.kind == EndOfFile => break,
                Err(e) => fail!("{}", e),
            }
        }

        assert_eq!(read.len(), data.len());
        assert_eq!(read, data);
    }
}
