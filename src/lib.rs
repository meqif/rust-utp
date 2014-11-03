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

#![feature(macro_rules)]
#![feature(phase)]
#![feature(if_let)]
#![feature(while_let)]
#![feature(globs)]
#![deny(missing_docs)]

extern crate time;
#[phase(plugin, link)] extern crate log;

use std::io::net::udp::UdpSocket;
use std::io::net::ip::SocketAddr;
use std::io::IoResult;
use std::rand::random;
use std::collections::DList;
use util::*;
use packet::*;

mod util;
mod bit_iterator;
mod packet;

// For simplicity's sake, let us assume no packet will ever exceed the
// Ethernet maximum transfer unit of 1500 bytes.
const BUF_SIZE: uint = 1500;
const DELAY_MAX_AGE: u32 = 2 * 60 * 1_000_000;
const GAIN: uint = 1;
const ALLOWED_INCREASE: uint = 1;
const TARGET: uint = 100_000; // 100 milliseconds
const MSS: uint = 1400;
const MIN_CWND: uint = 2;
const INIT_CWND: uint = 2;

macro_rules! iotry(
    ($e:expr) => (match $e { Ok(e) => e, Err(e) => panic!("{}", e) })
)

#[deriving(PartialEq,Eq,Show)]
enum UtpSocketState {
    SocketNew,
    SocketConnected,
    SocketSynSent,
    SocketFinReceived,
    SocketFinSent,
    SocketResetReceived,
    SocketClosed,
    SocketEndOfFile,
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
    fin_seq_nr: u16,

    rtt: int,
    rtt_variance: int,

    pending_data: Vec<u8>,
    curr_window: uint,
    remote_wnd_size: uint,

    current_delays: Vec<(u32,u32)>,
    base_delays: Vec<(u32,u32)>,
    congestion_timeout: u32,
    cwnd: uint,
}

impl UtpSocket {
    /// Create a UTP socket from the given address.
    #[unstable]
    pub fn bind(addr: SocketAddr) -> IoResult<UtpSocket> {
        let skt = UdpSocket::bind(addr);
        let connection_id = random::<u16>();
        match skt {
            Ok(x) => Ok(UtpSocket {
                socket: x,
                connected_to: addr,
                receiver_connection_id: connection_id,
                sender_connection_id: connection_id + 1,
                seq_nr: 1,
                ack_nr: 0,
                state: SocketNew,
                incoming_buffer: Vec::new(),
                send_window: Vec::new(),
                unsent_queue: DList::new(),
                duplicate_ack_count: 0,
                last_acked: 0,
                last_acked_timestamp: 0,
                fin_seq_nr: 0,
                rtt: 0,
                rtt_variance: 0,
                pending_data: Vec::new(),
                curr_window: 0,
                remote_wnd_size: 0,
                current_delays: Vec::new(),
                base_delays: Vec::new(),
                congestion_timeout: 1000, // 1 second
                cwnd: INIT_CWND * MSS,
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
        packet.set_type(SynPacket);
        packet.set_connection_id(self.receiver_connection_id);
        packet.set_seq_nr(self.seq_nr);

        let mut len = 0;
        let mut addr = self.connected_to;
        let mut buf = [0, ..BUF_SIZE];

        for _ in range(0u, 5) {
            packet.set_timestamp_microseconds(now_microseconds());

            // Send packet
            try!(self.socket.send_to(packet.bytes().as_slice(), other));
            self.state = SocketSynSent;

            // Validate response
            self.socket.set_read_timeout(Some(500));
            match self.socket.recv_from(buf) {
                Ok((read, src)) => { len = read; addr = src; break; },
                Err(ref e) if e.kind == std::io::TimedOut => continue,
                Err(e) => return Err(e),
            };
        }
        assert!(len == HEADER_SIZE);
        assert!(addr == self.connected_to);

        let packet = UtpPacket::decode(buf.slice_to(len));
        if packet.get_type() != StatePacket {
            return Err(IoError {
                kind: ConnectionFailed,
                desc: "The remote peer sent an invalid reply",
                detail: None,
            });
        }

        self.ack_nr = packet.seq_nr();
        self.state = SocketConnected;
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
            try!(self.recv_from(buf));
        }

        let mut packet = UtpPacket::new();
        packet.set_connection_id(self.sender_connection_id);
        packet.set_seq_nr(self.seq_nr);
        packet.set_ack_nr(self.ack_nr);
        packet.set_timestamp_microseconds(now_microseconds());
        packet.set_type(FinPacket);

        // Send FIN
        try!(self.socket.send_to(packet.bytes().as_slice(), self.connected_to));
        self.state = SocketFinSent;

        // Receive JAKE
        while self.state != SocketClosed {
            match self.recv_from(buf) {
                Ok(_) => {},
                Err(ref e) if e.kind == std::io::EndOfFile => self.state = SocketClosed,
                Err(e) => return Err(e),
            };
        }

        Ok(())
    }

    /// Receive data from socket.
    ///
    /// On success, returns the number of bytes read and the sender's address.
    /// Returns SocketEndOfFile after receiving a FIN packet when the remaining
    /// inflight packets are consumed. Subsequent calls return SocketClosed.
    #[unstable]
    pub fn recv_from(&mut self, buf: &mut[u8]) -> IoResult<(uint,SocketAddr)> {
        use std::io::{IoError, EndOfFile, Closed};

        if self.state == SocketEndOfFile {
            self.state = SocketClosed;
            return Err(IoError {
                kind: EndOfFile,
                desc: "End of file reached",
                detail: None,
            });
        }

        if self.state == SocketClosed {
            return Err(IoError {
                kind: Closed,
                desc: "Connection closed",
                detail: None,
            });
        }

        match self.flush_incoming_buffer(buf) {
            0 => self.recv(buf),
            read => Ok((read, self.connected_to)),
        }
    }

    fn recv(&mut self, buf: &mut[u8]) -> IoResult<(uint,SocketAddr)> {
        use std::io::{IoError, TimedOut, ConnectionReset};

        let mut b = [0, ..BUF_SIZE + HEADER_SIZE];
        if self.state != SocketNew {
            debug!("setting read timeout of {} ms", self.congestion_timeout);
            self.socket.set_read_timeout(Some(self.congestion_timeout as u64));
        }
        let (read, src) = match self.socket.recv_from(b) {
            Err(ref e) if e.kind == TimedOut => {
                debug!("recv_from timed out");
                self.congestion_timeout = self.congestion_timeout * 2;
                self.cwnd = MSS;
                self.send_fast_resend_request();
                return Ok((0, self.connected_to));
            },
            Ok(x) => x,
            Err(e) => return Err(e),
        };
        let packet = UtpPacket::decode(b.slice_to(read));
        debug!("received {}", packet);

        if packet.get_type() == ResetPacket {
            return Err(IoError {
                kind: ConnectionReset,
                desc: "Remote host aborted connection (incorrect connection id)",
                detail: None,
            });
        }

        if packet.get_type() == SynPacket {
            self.connected_to = src;
        }

        let shallow_clone = packet.shallow_clone();

        if packet.get_type() == DataPacket && self.ack_nr + 1 <= packet.seq_nr() {
            self.insert_into_buffer(packet);
        }

        if let Some(pkt) = self.handle_packet(shallow_clone) {
                let mut pkt = pkt;
                pkt.set_wnd_size(BUF_SIZE as u32);
                try!(self.socket.send_to(pkt.bytes().as_slice(), src));
                debug!("sent {}", pkt);
        }

        // Flush incoming buffer if possible
        let read = self.flush_incoming_buffer(buf);

        Ok((read, src))
    }

    #[allow(missing_docs)]
    #[deprecated = "renamed to `recv_from`"]
    pub fn recvfrom(&mut self, buf: &mut[u8]) -> IoResult<(uint,SocketAddr)> {
        self.recv_from(buf)
    }

    fn prepare_reply(&self, original: &UtpPacket, t: UtpPacketType) -> UtpPacket {
        let mut resp = UtpPacket::new();
        resp.set_type(t);
        let self_t_micro: u32 = now_microseconds();
        let other_t_micro: u32 = original.timestamp_microseconds();
        resp.set_timestamp_microseconds(self_t_micro);
        resp.set_timestamp_difference_microseconds((self_t_micro - other_t_micro));
        resp.set_connection_id(self.sender_connection_id);
        resp.set_seq_nr(self.seq_nr);
        resp.set_ack_nr(self.ack_nr);

        resp
    }

    /// Remove packet in incoming buffer and update current acknowledgement
    /// number.
    fn advance_incoming_buffer(&mut self) -> Option<UtpPacket> {
        match self.incoming_buffer.remove(0) {
            Some(packet) => {
                debug!("Removed packet from incoming buffer: {}", packet);
                self.ack_nr = packet.seq_nr();
                Some(packet)
            },
            None => None
        }
    }

    /// Discards sequential, ordered packets in incoming buffer, starting from
    /// the most recently acknowledged to the most recent, as long as there are
    /// no missing packets. The discarded packets' payload is written to the
    /// slice `buf`, starting in position `start`.
    /// Returns the last written index.
    fn flush_incoming_buffer(&mut self, buf: &mut [u8]) -> uint {
        let mut idx = 0;

        // Check if there is any pending data from a partially flushed packet
        if !self.pending_data.is_empty() {
            let len = buf.clone_from_slice(self.pending_data.as_slice());

            // If all the data in the pending data buffer fits the given output
            // buffer, remove the corresponding packet from the incoming buffer
            // and clear the pending data buffer
            if len == self.pending_data.len() {
                self.pending_data.clear();
                self.advance_incoming_buffer();
                return idx + len;
            } else {
                // Remove the bytes copied to the output buffer from the pending
                // data buffer (i.e., pending -= output)
                self.pending_data = self.pending_data.slice_from(len).to_vec();
            }
        }

        // Copy the payload of as many packets in the incoming buffer as possible
        while !self.incoming_buffer.is_empty() &&
            (self.ack_nr == self.incoming_buffer[0].seq_nr() ||
             self.ack_nr + 1 == self.incoming_buffer[0].seq_nr())
        {
            let len = std::cmp::min(buf.len() - idx, self.incoming_buffer[0].payload.len());

            for i in range(0, len) {
                buf[idx] = self.incoming_buffer[0].payload[i];
                idx += 1;
            }

            // Remove top packet if its payload fits the output buffer
            if self.incoming_buffer[0].payload.len() == len {
                self.advance_incoming_buffer();
            } else {
                self.pending_data.push_all(self.incoming_buffer[0].payload.slice_from(len));
            }

            // Stop if the output buffer is full
            if buf.len() == idx {
                return idx;
            }
        }

        return idx;
    }

    /// Send data on socket to the remote peer. Returns nothing on success.
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
    pub fn send_to(&mut self, buf: &[u8]) -> IoResult<()> {
        use std::io::{IoError, Closed};

        if self.state == SocketClosed {
            return Err(IoError {
                kind: Closed,
                desc: "Connection closed",
                detail: None,
            });
        }

        for chunk in buf.chunks(MSS - HEADER_SIZE) {
            let mut packet = UtpPacket::new();
            packet.set_type(DataPacket);
            packet.payload = chunk.to_vec();
            packet.set_timestamp_microseconds(now_microseconds());
            packet.set_seq_nr(self.seq_nr);
            packet.set_ack_nr(self.ack_nr);
            packet.set_connection_id(self.sender_connection_id);

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
        while let Some(packet) = self.unsent_queue.pop_front() {
            debug!("current window: {}", self.send_window.len());
            let max_inflight = std::cmp::min(self.cwnd, self.remote_wnd_size);
            let max_inflight = std::cmp::max(MIN_CWND * MSS, max_inflight);
            while self.curr_window + packet.len() > max_inflight {
                let mut buf = [0, ..BUF_SIZE];
                iotry!(self.recv_from(buf));
            }

            iotry!(self.socket.send_to(packet.bytes().as_slice(), dst));
            debug!("sent {}", packet);
            self.curr_window += packet.len();
            self.send_window.push(packet);
        }
    }


    #[allow(missing_docs)]
    #[deprecated = "renamed to `send_to`"]
    pub fn sendto(&mut self, buf: &[u8]) -> IoResult<()> {
        self.send_to(buf)
    }

    /// Send fast resend request.
    ///
    /// Sends three identical ACK/STATE packets to the remote host, signalling a
    /// fast resend request.
    fn send_fast_resend_request(&mut self) {
        let mut packet = UtpPacket::new();
        packet.set_wnd_size(BUF_SIZE as u32);
        packet.set_type(StatePacket);
        packet.set_ack_nr(self.ack_nr);
        packet.set_seq_nr(self.seq_nr);
        packet.set_connection_id(self.sender_connection_id);

        for _ in range(0u, 3) {
            let t = now_microseconds();
            packet.set_timestamp_microseconds(t);
            packet.set_timestamp_difference_microseconds((t - self.last_acked_timestamp));
            iotry!(self.socket.send_to(packet.bytes().as_slice(), self.connected_to));
            debug!("sent {}", packet);
        }
    }

    fn update_base_delay(&mut self, v: u32) {
        // Remove measurements more than 2 minutes old
        let now = now_microseconds();
        while !self.base_delays.is_empty() && now - self.base_delays[0].val0() > DELAY_MAX_AGE {
            self.base_delays.remove(0);
        }

        // Insert new measurement
        self.base_delays.push((now_microseconds(), v));
    }

    fn update_current_delay(&mut self, v: u32) {
        // Remove measurements more than 2 minutes old
        let now = now_microseconds();
        while !self.current_delays.is_empty() && now - self.current_delays[0].val0() > DELAY_MAX_AGE {
            self.current_delays.remove(0);
        }

        // Insert new measurement
        self.current_delays.push((now_microseconds(), v));
    }

    fn update_congestion_timeout(&mut self, current_delay: int) {
        let delta = self.rtt - current_delay;
        self.rtt_variance += (std::num::abs(delta) - self.rtt_variance) / 4;
        self.rtt += (current_delay - self.rtt) / 8;
        self.congestion_timeout = std::cmp::max(self.rtt + self.rtt_variance * 4, 500) as u32;
        self.congestion_timeout = std::cmp::min(self.congestion_timeout, 60_000);

        debug!("current_delay: {}", current_delay);
        debug!("delta: {}", delta);
        debug!("self.rtt_variance: {}", self.rtt_variance);
        debug!("self.rtt: {}", self.rtt);
        debug!("self.congestion_timeout: {}", self.congestion_timeout);
    }

    /// Calculate the filtered current delay in the current window.
    ///
    /// The current delay is calculated through application of the exponential
    /// weighted moving average filter with smoothing factor 0.333 over the
    /// current delays in the current window.
    fn filtered_current_delay(&self) -> u32 {
        let input = self.current_delays.iter().map(|&(_,x)| x as f64).collect();
        let output = exponential_weighted_moving_average(input, 0.333);
        output[output.len() - 1] as u32
    }

    /// Calculate the lowest base delay in the current window.
    fn min_base_delay(&self) -> u32 {
        self.base_delays.iter().min().unwrap().val1()
    }

    /// Build the selective acknowledgment payload for usage in packets.
    fn build_selective_ack(&self) -> Vec<u8> {
        let mut stashed = self.incoming_buffer.iter()
            .filter(|&pkt| pkt.seq_nr() > self.ack_nr);

        let mut sack = Vec::new();
        for packet in stashed {
            let diff = packet.seq_nr() - self.ack_nr - 2;
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
            sack.grow((len / 4 + 1) * 4 - len, 0);
        }

        return sack;
    }

    fn resend_lost_packet(&mut self, lost_packet_nr: u16) {
        match self.send_window.iter().find(|pkt| pkt.seq_nr() == lost_packet_nr) {
            None => debug!("Packet {} not found", lost_packet_nr),
            Some(packet) => {
                iotry!(self.socket.send_to(packet.bytes().as_slice(), self.connected_to));
                debug!("sent {}", packet);
            }
        }
    }

    /// Forget sent packets that were acknowledged by the remote peer.
    fn advance_send_window(&mut self) {
        if let Some(position) = self.send_window.iter()
            .position(|pkt| pkt.seq_nr() == self.last_acked)
        {
            for _ in range(0, position + 1) {
                let packet = self.send_window.remove(0).unwrap();
                self.curr_window -= packet.len();
            }
        }
        debug!("self.curr_window: {}", self.curr_window);
    }

    /// Handle incoming packet, updating socket state accordingly.
    ///
    /// Returns appropriate reply packet, if needed.
    fn handle_packet(&mut self, packet: UtpPacket) -> Option<UtpPacket> {
        // Reset connection if connection id doesn't match and this isn't a SYN
        if packet.get_type() != SynPacket &&
           !(packet.connection_id() == self.sender_connection_id ||
           packet.connection_id() == self.receiver_connection_id) {
            return Some(self.prepare_reply(&packet, ResetPacket));
        }

        // Acknowledge only if the packet strictly follows the previous one
        if self.ack_nr + 1 == packet.seq_nr() {
            self.ack_nr = packet.seq_nr();
        }

        self.remote_wnd_size = packet.wnd_size() as uint;
        debug!("self.remote_wnd_size: {}", self.remote_wnd_size);

        // Ignore packets with sequence number higher than the one in the FIN packet.
        if self.state == SocketFinReceived && self.fin_seq_nr == self.ack_nr &&
            packet.seq_nr() > self.fin_seq_nr
        {
            debug!("Ignoring packet with sequence number {} (higher than FIN: {})",
                   packet.seq_nr(), self.fin_seq_nr);
            return None;
        }

        match packet.get_type() {
            SynPacket => { // Respond with an ACK and populate own fields
                // Update socket information for new connections
                self.ack_nr = packet.seq_nr();
                self.seq_nr = random();
                self.receiver_connection_id = packet.connection_id() + 1;
                self.sender_connection_id = packet.connection_id();
                self.state = SocketConnected;

                Some(self.prepare_reply(&packet, StatePacket))
            }
            DataPacket => {
                let mut reply = self.prepare_reply(&packet, StatePacket);

                if self.ack_nr + 1 < packet.seq_nr() {
                    debug!("current ack_nr ({}) is behind received packet seq_nr ({})",
                           self.ack_nr, packet.seq_nr());

                    // Set SACK extension payload if the packet is not in order
                    let sack = self.build_selective_ack();

                    if sack.len() > 0 {
                        reply.set_sack(Some(sack));
                    }
                }

                Some(reply)
            },
            FinPacket => {
                self.state = SocketFinReceived;
                self.fin_seq_nr = packet.seq_nr();

                // If all packets are received and handled
                if self.pending_data.is_empty() &&
                    self.incoming_buffer.is_empty() &&
                    self.ack_nr == self.fin_seq_nr
                {
                    self.state = SocketEndOfFile;
                    Some(self.prepare_reply(&packet, StatePacket))
                } else {
                    debug!("FIN received but there are missing packets");
                    None
                }
            }
            StatePacket => {
                if packet.ack_nr() == self.last_acked {
                    self.duplicate_ack_count += 1;
                } else {
                    self.last_acked = packet.ack_nr();
                    self.last_acked_timestamp = now_microseconds();
                    self.duplicate_ack_count = 1;
                }

                self.update_base_delay(packet.timestamp_microseconds());
                self.update_current_delay(packet.timestamp_difference_microseconds());

                let bytes_newly_acked = packet.len();
                let flightsize = self.curr_window;

                let queuing_delay = self.filtered_current_delay() - self.min_base_delay();
                let target = TARGET as u32;
                let off_target: u32 = (target - queuing_delay) / target;
                self.cwnd += GAIN * off_target as uint * bytes_newly_acked * MSS / self.cwnd;
                let max_allowed_cwnd = flightsize + ALLOWED_INCREASE * MSS;
                self.cwnd = std::cmp::min(self.cwnd, max_allowed_cwnd);
                self.cwnd = std::cmp::max(self.cwnd, MIN_CWND * MSS);

                let rtt = (target - off_target) / 1000; // in milliseconds
                self.update_congestion_timeout(rtt as int);

                debug!("queuing_delay: {}", queuing_delay);
                debug!("off_target: {}", off_target);
                debug!("cwnd: {}", self.cwnd);
                debug!("max_allowed_cwnd: {}", max_allowed_cwnd);

                let mut packet_loss_detected: bool = !self.send_window.is_empty() &&
                                                     self.duplicate_ack_count == 3;

                // Process extensions, if any
                for extension in packet.extensions.iter() {
                    if extension.get_type() == SelectiveAckExtension {
                        let bits = extension.iter();
                        // If three or more packets are acknowledged past the implicit missing one,
                        // assume it was lost.
                        if bits.filter(|&bit| bit == 1).count() >= 3 {
                            self.resend_lost_packet(packet.ack_nr() + 1);
                            packet_loss_detected = true;
                        }

                        let bits = extension.iter();
                        for (idx, received) in bits.map(|bit| bit == 1).enumerate() {
                            let seq_nr = packet.ack_nr() + 2 + idx as u16;
                            if received {
                                debug!("SACK: packet {} received", seq_nr);
                            } else if !self.send_window.is_empty() &&
                                seq_nr < self.send_window.last().unwrap().seq_nr()
                            {
                                debug!("SACK: packet {} lost", seq_nr);
                                self.resend_lost_packet(seq_nr);
                                packet_loss_detected = true;
                            } else {
                                break;
                            }
                        }
                    } else {
                        debug!("Unknown extension {}, ignoring", extension.get_type());
                    }
                }

                // Packet lost, halve the congestion window
                if packet_loss_detected {
                    debug!("packet loss detected, halving congestion window");
                    self.cwnd = std::cmp::max(self.cwnd / 2, MIN_CWND * MSS);
                    debug!("cwnd: {}", self.cwnd);
                }

                // Three duplicate ACKs, must resend packets since `ack_nr + 1`
                // TODO: checking if the send buffer isn't empty isn't a
                // foolproof way to differentiate between triple-ACK and three
                // keep alives spread in time
                if !self.send_window.is_empty() && self.duplicate_ack_count == 3 {
                    for i in range(0, self.send_window.len()) {
                        let seq_nr = self.send_window[i].seq_nr();
                        if seq_nr <= packet.ack_nr() { continue; }
                        self.resend_lost_packet(seq_nr);
                    }
                }

                // Success, advance send window
                self.advance_send_window();

                if self.state == SocketFinSent && packet.ack_nr() == self.seq_nr {
                    self.state = SocketClosed;
                }

                None
            },
            ResetPacket => {
                self.state = SocketResetReceived;
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
            self.incoming_buffer[i].seq_nr() == packet.seq_nr() {
            self.incoming_buffer.remove(i);
        }
        self.incoming_buffer.insert(i, packet);
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
        match UtpSocket::bind(addr) {
            Ok(s)  => Ok(UtpStream { socket: s }),
            Err(e) => Err(e),
        }
    }

    /// Open a uTP connection to a remote host by hostname or IP address.
    #[unstable]
    pub fn connect(dst: SocketAddr) -> IoResult<UtpStream> {
        use std::io::net::ip::Ipv4Addr;

        // Port 0 means the operating system gets to choose it
        let my_addr = SocketAddr { ip: Ipv4Addr(0,0,0,0), port: 0 };
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
        self.socket.send_to(buf)
    }
}

#[cfg(test)]
mod test {
    use super::{UtpSocket, UtpStream};
    use super::{BUF_SIZE};
    use super::{SocketConnected, SocketNew, SocketClosed, SocketEndOfFile};
    use std::rand::random;
    use std::io::test::next_test_ip4;
    use util::now_microseconds;
    use packet::{UtpPacket, StatePacket, FinPacket, DataPacket, ResetPacket, SynPacket};

    #[test]
    fn test_socket_ipv4() {
        let (server_addr, client_addr) = (next_test_ip4(), next_test_ip4());

        let client = iotry!(UtpSocket::bind(client_addr));
        let mut server = iotry!(UtpSocket::bind(server_addr));

        assert!(server.state == SocketNew);
        assert!(client.state == SocketNew);

        // Check proper difference in client's send connection id and receive connection id
        assert_eq!(client.sender_connection_id, client.receiver_connection_id + 1);

        spawn(proc() {
            let client = iotry!(client.connect(server_addr));
            assert!(client.state == SocketConnected);
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

        assert!(server.state == SocketConnected);
        drop(server);
    }

    #[test]
    fn test_recvfrom_on_closed_socket() {
        use std::io::{Closed, EndOfFile};

        let (server_addr, client_addr) = (next_test_ip4(), next_test_ip4());

        let client = iotry!(UtpSocket::bind(client_addr));
        let mut server = iotry!(UtpSocket::bind(server_addr));

        assert!(server.state == SocketNew);
        assert!(client.state == SocketNew);

        spawn(proc() {
            let mut client = iotry!(client.connect(server_addr));
            assert!(client.state == SocketConnected);
            assert_eq!(client.close(), Ok(()));
            drop(client);
        });

        // Make the server listen for incoming connections
        let mut buf = [0u8, ..BUF_SIZE];
        let _resp = server.recv_from(buf);
        assert!(server.state == SocketConnected);

        // Closing the connection is fine
        match server.recv_from(buf) {
            Err(e) => panic!("{}", e),
            _ => {},
        }
        assert_eq!(server.state, SocketEndOfFile);

        // Trying to listen on the socket after closing it raises an
        // EOF error
        match server.recv_from(buf) {
            Err(e) => assert_eq!(e.kind, EndOfFile),
            v => panic!("expected {}, got {}", EndOfFile, v),
        }

        assert_eq!(server.state, SocketClosed);

        // Trying again raises a Closed error
        match server.recv_from(buf) {
            Err(e) => assert_eq!(e.kind, Closed),
            v => panic!("expected {}, got {}", Closed, v),
        }

        drop(server);
    }

    #[test]
    fn test_sendto_on_closed_socket() {
        use std::io::Closed;

        let (server_addr, client_addr) = (next_test_ip4(), next_test_ip4());

        let client = iotry!(UtpSocket::bind(client_addr));
        let mut server = iotry!(UtpSocket::bind(server_addr));

        assert!(server.state == SocketNew);
        assert!(client.state == SocketNew);

        spawn(proc() {
            let client = iotry!(client.connect(server_addr));
            assert!(client.state == SocketConnected);
            let mut buf = [0u8, ..BUF_SIZE];
            let mut client = client;
            iotry!(client.recv_from(buf));
        });

        // Make the server listen for incoming connections
        let mut buf = [0u8, ..BUF_SIZE];
        let (_read, _src) = iotry!(server.recv_from(buf));
        assert!(server.state == SocketConnected);

        iotry!(server.close());
        assert_eq!(server.state, SocketClosed);

        // Trying to send to the socket after closing it raises an
        // error
        match server.send_to(buf) {
            Err(e) => assert_eq!(e.kind, Closed),
            v => panic!("expected {}, got {}", Closed, v),
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
        assert!(client.state == SocketConnected);
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
        packet.set_type(SynPacket);
        packet.set_connection_id(initial_connection_id);

        // Do we have a response?
        let response = socket.handle_packet(packet.clone());
        assert!(response.is_some());

        // Is is of the correct type?
        let response = response.unwrap();
        assert!(response.get_type() == StatePacket);

        // Same connection id on both ends during connection establishment
        assert!(response.connection_id() == packet.connection_id());

        // Response acknowledges SYN
        assert!(response.ack_nr() == packet.seq_nr());

        // No payload?
        assert!(response.payload.is_empty());
        //}

        // ---------------------------------

        // fn test_connection_usage() {
        let old_packet = packet;
        let old_response = response;

        let mut packet = UtpPacket::new();
        packet.set_type(DataPacket);
        packet.set_connection_id(sender_connection_id);
        packet.set_seq_nr(old_packet.seq_nr() + 1);
        packet.set_ack_nr(old_response.seq_nr());

        let response = socket.handle_packet(packet.clone());
        assert!(response.is_some());

        let response = response.unwrap();
        assert!(response.get_type() == StatePacket);

        // Sender (i.e., who initated connection and sent SYN) has connection id
        // equal to initial connection id + 1
        // Receiver (i.e., who accepted connection) has connection id equal to
        // initial connection id
        assert!(response.connection_id() == initial_connection_id);
        assert!(response.connection_id() == packet.connection_id() - 1);

        // Previous packets should be ack'ed
        assert!(response.ack_nr() == packet.seq_nr());

        // Responses with no payload should not increase the sequence number
        assert!(response.payload.is_empty());
        assert!(response.seq_nr() == old_response.seq_nr());
        // }

        //fn test_connection_teardown() {
        let old_packet = packet;
        let old_response = response;

        let mut packet = UtpPacket::new();
        packet.set_type(FinPacket);
        packet.set_connection_id(sender_connection_id);
        packet.set_seq_nr(old_packet.seq_nr() + 1);
        packet.set_ack_nr(old_response.seq_nr());

        let response = socket.handle_packet(packet.clone());
        assert!(response.is_some());

        let response = response.unwrap();

        assert!(response.get_type() == StatePacket);

        // FIN packets have no payload but the sequence number shouldn't increase
        assert!(packet.seq_nr() == old_packet.seq_nr() + 1);

        // Nor should the ACK packet's sequence number
        assert!(response.seq_nr() == old_response.seq_nr());

        // FIN should be acknowledged
        assert!(response.ack_nr() == packet.seq_nr());

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
        packet.set_type(SynPacket);
        packet.set_connection_id(initial_connection_id);

        let response = socket.handle_packet(packet.clone());
        assert!(response.is_some());
        let response = response.unwrap();
        assert!(response.get_type() == StatePacket);

        let old_packet = packet;
        let old_response = response;

        // Now, send a keepalive packet
        let mut packet = UtpPacket::new();
        packet.set_wnd_size(BUF_SIZE as u32);
        packet.set_type(StatePacket);
        packet.set_connection_id(initial_connection_id);
        packet.set_seq_nr(old_packet.seq_nr() + 1);
        packet.set_ack_nr(old_response.seq_nr());

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
        packet.set_type(SynPacket);
        packet.set_connection_id(initial_connection_id);

        let response = socket.handle_packet(packet.clone());
        assert!(response.is_some());
        assert!(response.unwrap().get_type() == StatePacket);

        // Now, disrupt connection with a packet with an incorrect connection id
        let new_connection_id = initial_connection_id * 2;

        let mut packet = UtpPacket::new();
        packet.set_wnd_size(BUF_SIZE as u32);
        packet.set_type(StatePacket);
        packet.set_connection_id(new_connection_id);

        let response = socket.handle_packet(packet.clone());
        assert!(response.is_some());

        let response = response.unwrap();
        assert!(response.get_type() == ResetPacket);
        assert!(response.ack_nr() == packet.seq_nr());
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
        const LEN: uint = 1024;
        let data = Vec::from_fn(LEN, |idx| idx as u8);
        assert_eq!(LEN, data.len());

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
        assert_eq!(read.len(), data.len());
        assert_eq!(read, data);
    }

    #[test]
    fn test_utp_stream_large_data() {
        // Has to be sent over several packets
        const LEN: uint = 1024 * 1024;
        let data = Vec::from_fn(LEN, |idx| idx as u8);
        assert_eq!(LEN, data.len());

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
        assert_eq!(read.len(), data.len());
        assert_eq!(read, data);
    }

    #[test]
    fn test_utp_stream_successive_reads() {
        use std::io::Closed;

        const LEN: uint = 1024;
        let data: Vec<u8> = Vec::from_fn(LEN, |idx| idx as u8);
        assert_eq!(LEN, data.len());

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
            _ => panic!("should have failed with Closed"),
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
        packet.set_type(SynPacket);
        packet.set_connection_id(initial_connection_id);

        let response = socket.handle_packet(packet.clone());
        assert!(response.is_some());
        let response = response.unwrap();
        assert!(response.get_type() == StatePacket);

        let old_packet = packet;
        let old_response = response;

        let mut window: Vec<UtpPacket> = Vec::new();

        // Now, send a keepalive packet
        let mut packet = UtpPacket::new();
        packet.set_wnd_size(BUF_SIZE as u32);
        packet.set_type(DataPacket);
        packet.set_connection_id(initial_connection_id);
        packet.set_seq_nr(old_packet.seq_nr() + 1);
        packet.set_ack_nr(old_response.seq_nr());
        packet.payload = vec!(1,2,3);
        window.push(packet);

        let mut packet = UtpPacket::new();
        packet.set_wnd_size(BUF_SIZE as u32);
        packet.set_type(DataPacket);
        packet.set_connection_id(initial_connection_id);
        packet.set_seq_nr(old_packet.seq_nr() + 2);
        packet.set_ack_nr(old_response.seq_nr());
        packet.payload = vec!(4,5,6);
        window.push(packet);

        // Send packets in reverse order
        let response = socket.handle_packet(window[1].clone());
        assert!(response.is_some());
        let response = response.unwrap();
        assert!(response.ack_nr() != window[1].seq_nr());

        let response = socket.handle_packet(window[0].clone());
        assert!(response.is_some());
    }

    #[test]
    fn test_socket_unordered_packets() {
        let (server_addr, client_addr) = (next_test_ip4(), next_test_ip4());

        let client = iotry!(UtpSocket::bind(client_addr));
        let mut server = iotry!(UtpSocket::bind(server_addr));

        assert!(server.state == SocketNew);
        assert!(client.state == SocketNew);

        // Check proper difference in client's send connection id and receive connection id
        assert_eq!(client.sender_connection_id, client.receiver_connection_id + 1);

        spawn(proc() {
            let mut client = iotry!(client.connect(server_addr));
            assert!(client.state == SocketConnected);
            let mut s = client.socket;
            let mut window: Vec<UtpPacket> = Vec::new();

            for data in Vec::from_fn(12, |idx| idx as u8 + 1).as_slice().chunks(3) {
                let mut packet = UtpPacket::new();
                packet.set_wnd_size(BUF_SIZE as u32);
                packet.set_type(DataPacket);
                packet.set_connection_id(client.sender_connection_id);
                packet.set_seq_nr(client.seq_nr);
                packet.set_ack_nr(client.ack_nr);
                packet.payload = data.to_vec();
                window.push(packet.clone());
                client.send_window.push(packet.clone());
                client.seq_nr += 1;
            }

            let mut packet = UtpPacket::new();
            packet.set_wnd_size(BUF_SIZE as u32);
            packet.set_type(FinPacket);
            packet.set_connection_id(client.sender_connection_id);
            packet.set_seq_nr(client.seq_nr);
            packet.set_ack_nr(client.ack_nr);
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

        assert!(server.state == SocketConnected);

        let mut stream = UtpStream { socket: server };
        let expected: Vec<u8> = Vec::from_fn(12, |idx| idx as u8 + 1);

        match stream.read_to_end() {
            Ok(data) => {
                assert_eq!(data.len(), expected.len());
                assert_eq!(data, expected);
            },
            Err(e) => panic!("{}", e),
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
                Err(e) => panic!("{}", e),
            };
            assert_eq!(packet.ack_nr(), seq_nr);
            drop(client);
        });

        let mut server = server;
        let mut buf = [0, ..20];
        iotry!(server.recv_from(buf));
        assert!(server.ack_nr != 0);
        assert_eq!(server.ack_nr, seq_nr);
        assert!(server.incoming_buffer.is_empty());
    }

    #[test]
    fn test_response_to_triple_ack() {
        let (server_addr, client_addr) = (next_test_ip4(), next_test_ip4());
        let mut server = iotry!(UtpSocket::bind(server_addr));
        let client = iotry!(UtpSocket::bind(client_addr));

        // Fits in a packet
        const LEN: uint = 1024;
        let data = Vec::from_fn(LEN, |idx| idx as u8);
        let d = data.clone();
        assert_eq!(LEN, data.len());

        spawn(proc() {
            let mut client = iotry!(client.connect(server_addr));
            iotry!(client.send_to(d.as_slice()));
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
                assert!(data_packet.get_type() == DataPacket);
                assert_eq!(data_packet.payload, data);
                assert_eq!(data_packet.payload.len(), data.len());
            },
            Err(e) => panic!("{}", e),
        }
        let data_packet = data_packet;

        // Send triple ACK
        let mut packet = UtpPacket::new();
        packet.set_wnd_size(BUF_SIZE as u32);
        packet.set_type(StatePacket);
        packet.set_seq_nr(server.seq_nr);
        packet.set_ack_nr(data_packet.seq_nr() - 1);
        packet.set_connection_id(server.sender_connection_id);

        for _ in range(0u, 3) {
            iotry!(server.socket.send_to(packet.bytes().as_slice(), client_addr));
        }

        // Receive data again and check that it's the same we reported as missing
        match server.socket.recv_from(buf) {
            Ok((0, _)) => panic!("Received 0 bytes from socket"),
            Ok((read, _src)) => {
                let packet = UtpPacket::decode(buf.slice_to(read));
                assert_eq!(packet.get_type(), DataPacket);
                assert_eq!(packet.seq_nr(), data_packet.seq_nr());
                assert!(packet.payload == data_packet.payload);
                let response = server.handle_packet(packet).unwrap();
                iotry!(server.socket.send_to(response.bytes().as_slice(), server.connected_to));
            },
            Err(e) => panic!("{}", e),
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

        assert!(server.state == SocketNew);
        assert!(client.state == SocketNew);

        // Check proper difference in client's send connection id and receive connection id
        assert_eq!(client.sender_connection_id, client.receiver_connection_id + 1);

        spawn(proc() {
            let mut client = iotry!(client.connect(server_addr));
            assert!(client.state == SocketConnected);
            assert_eq!(client.connected_to, server_addr);
            iotry!(client.send_to(d.as_slice()));
            drop(client);
        });

        let mut buf = [0u8, ..BUF_SIZE];
        match server.recv_from(buf) {
            e => println!("{}", e),
        }
        // After establishing a new connection, the server's ids are a mirror of the client's.
        assert_eq!(server.receiver_connection_id, server.sender_connection_id + 1);
        assert_eq!(server.connected_to, client_addr);

        assert!(server.state == SocketConnected);

        // Purposefully read from UDP socket directly and discard it, in order
        // to behave as if the packet was lost and thus trigger the timeout
        // handling in the *next* call to `UtpSocket.recv_from`.
        iotry!(server.socket.recv_from(buf));

        // Set a much smaller than usual timeout, for quicker test completion
        server.congestion_timeout = 50;

        // Now wait for the previously discarded packet
        loop {
            match server.recv_from(buf) {
                Ok((0, _)) => continue,
                Ok(_) => break,
                Err(e) => panic!("{}", e),
            }
        }

        drop(server);
    }

    #[test]
    fn test_sorted_buffer_insertion() {
        let server_addr = next_test_ip4();
        let mut socket = iotry!(UtpSocket::bind(server_addr));

        let mut packet = UtpPacket::new();
        packet.set_seq_nr(1);

        assert!(socket.incoming_buffer.is_empty());

        socket.insert_into_buffer(packet.clone());
        assert_eq!(socket.incoming_buffer.len(), 1);

        packet.set_seq_nr(2);
        packet.set_timestamp_microseconds(128);

        socket.insert_into_buffer(packet.clone());
        assert_eq!(socket.incoming_buffer.len(), 2);
        assert_eq!(socket.incoming_buffer[1].seq_nr(), 2);
        assert_eq!(socket.incoming_buffer[1].timestamp_microseconds(), 128);

        packet.set_seq_nr(3);
        packet.set_timestamp_microseconds(256);

        socket.insert_into_buffer(packet.clone());
        assert_eq!(socket.incoming_buffer.len(), 3);
        assert_eq!(socket.incoming_buffer[2].seq_nr(), 3);
        assert_eq!(socket.incoming_buffer[2].timestamp_microseconds(), 256);

        // Replace a packet with a more recent version
        packet.set_seq_nr(2);
        packet.set_timestamp_microseconds(456);

        socket.insert_into_buffer(packet.clone());
        assert_eq!(socket.incoming_buffer.len(), 3);
        assert_eq!(socket.incoming_buffer[1].seq_nr(), 2);
        assert_eq!(socket.incoming_buffer[1].timestamp_microseconds(), 456);
    }

    #[test]
    fn test_duplicate_packet_handling() {
        let (server_addr, client_addr) = (next_test_ip4(), next_test_ip4());

        let client = iotry!(UtpSocket::bind(client_addr));
        let mut server = iotry!(UtpSocket::bind(server_addr));

        assert!(server.state == SocketNew);
        assert!(client.state == SocketNew);

        // Check proper difference in client's send connection id and receive connection id
        assert_eq!(client.sender_connection_id, client.receiver_connection_id + 1);

        spawn(proc() {
            let mut client = iotry!(client.connect(server_addr));
            assert!(client.state == SocketConnected);
            let mut s = client.socket.clone();

            let mut packet = UtpPacket::new();
            packet.set_wnd_size(BUF_SIZE as u32);
            packet.set_type(DataPacket);
            packet.set_connection_id(client.sender_connection_id);
            packet.set_seq_nr(client.seq_nr);
            packet.set_ack_nr(client.ack_nr);
            packet.payload = vec!(1,2,3);

            // Send two copies of the packet, with different timestamps
            for _ in range(0u, 2) {
                packet.set_timestamp_microseconds(now_microseconds());
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

        assert!(server.state == SocketConnected);

        let mut stream = UtpStream { socket: server };
        let expected: Vec<u8> = vec!(1,2,3);

        match stream.read_to_end() {
            Ok(data) => {
                println!("{}", data);
                assert_eq!(data.len(), expected.len());
                assert_eq!(data, expected);
            },
            Err(e) => panic!("{}", e),
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
            client.socket.congestion_timeout = 50;

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
        packet.set_seq_nr(server.seq_nr);
        packet.set_ack_nr(server.ack_nr - 1);
        packet.set_connection_id(server.sender_connection_id);
        packet.set_timestamp_microseconds(now_microseconds());
        packet.set_type(StatePacket);
        packet.set_sack(Some(vec!(12, 0, 0, 0)));

        // Send SACK
        iotry!(server.socket.send_to(packet.bytes().as_slice(), server.connected_to.clone()));

        // Expect to receive "missing" packets
        let mut stream = UtpStream { socket: server };
        let read = iotry!(stream.read_to_end());
        assert!(!read.is_empty());
        assert_eq!(read.len(), data.len());
        assert_eq!(read, data);
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
                packet.set_seq_nr(client.seq_nr);
                packet.set_ack_nr(client.ack_nr);
                packet.set_connection_id(client.sender_connection_id);
                packet.set_timestamp_microseconds(now_microseconds());
                packet.payload = chunk.to_vec();
                packet.set_type(DataPacket);

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
        while server.state != SocketClosed {
            let mut small_buffer = [0, ..512];
            match server.recv_from(small_buffer) {
                Ok((0, _src)) => (),
                Ok((len, _src)) => read.push_all(small_buffer.slice_to(len)),
                Err(ref e) if e.kind == EndOfFile => break,
                Err(e) => panic!("{}", e),
            }
        }

        assert_eq!(read.len(), data.len());
        assert_eq!(read, data);
    }

    #[test]
    fn test_sequence_number_rollover() {
        let (server_addr, client_addr) = (next_test_ip4(), next_test_ip4());

        let mut server = UtpStream::bind(server_addr);

        let len = BUF_SIZE * 4;
        let data = Vec::from_fn(len, |idx| idx as u8);
        let to_send = data.clone();

        spawn(proc() {
            let mut socket = iotry!(UtpSocket::bind(client_addr));

            // Advance socket's sequence number
            socket.seq_nr = ::std::u16::MAX - (to_send.len() / (BUF_SIZE * 2)) as u16;

            let socket = iotry!(socket.connect(server_addr));
            let mut client = UtpStream { socket: socket };
            // Send enough data to rollover
            iotry!(client.write(to_send.as_slice()));
            // Check that the sequence number did rollover
            assert!(client.socket.seq_nr < 50);
            // Close connection
            iotry!(client.close());
        });

        let received = iotry!(server.read_to_end());
        assert_eq!(received.len(), data.len());
        assert_eq!(received, data);
    }
}
