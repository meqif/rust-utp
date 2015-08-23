#![allow(dead_code)]

use std::error::Error;
use std::mem::transmute;
use std::fmt;
use std::ops::Deref;
use bit_iterator::BitIterator;

pub const HEADER_SIZE: usize = 20;

macro_rules! u8_to_unsigned_be {
    ($src:ident, $start:expr, $end:expr, $t:ty) => ({
        (0 .. $end - $start + 1).rev().fold(0, |acc, i| acc | $src[$start+i] as $t << i * 8)
    })
}

macro_rules! make_getter {
    ($name:ident, $t:ty, $m:ident) => {
        pub fn $name(&self) -> $t {
            $m::from_be(self.header.$name)
        }
    }
}

macro_rules! make_setter {
    ($fn_name:ident, $field:ident, $t: ty) => {
        pub fn $fn_name(&mut self, new: $t) {
            self.header.$field = new.to_be();
        }
    }
}

/// A trait for objects that can be represented as a vector of bytes.
pub trait Encodable {
    /// Returns a vector of bytes representing the data structure in a way that can be sent over the
    /// network.
    fn to_bytes(&self) -> Vec<u8>;
}

/// A trait for objects that can be decoded from slices of bytes.
pub trait Decodable: Sized {
    /// Decodes a slice of bytes and returns an equivalent object.
    ///
    /// If the slice of bytes represents a valid instance of the type, it returns `Ok`, containing
    /// the corresponding object; if a parse error is found, it returns `Err` with the appropriate
    /// `ParseError`.
    fn from_bytes(&[u8]) -> Result<Self, ParseError>;
}

#[derive(Debug)]
pub enum ParseError {
    InvalidExtensionLength,
    InvalidPacketLength,
    InvalidPacketType,
    UnsupportedVersion
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.description())
    }
}

impl Error for ParseError {
    fn description(&self) -> &str {
        use self::ParseError::*;
        match *self {
            InvalidExtensionLength => "Invalid extension length (must be a non-zero multiple of 4)",
            InvalidPacketLength => "The packet is too small",
            InvalidPacketType => "Invalid packet type",
            UnsupportedVersion => "Unsupported packet version",
        }
    }
}

#[derive(PartialEq, Eq, Debug)]
pub enum PacketType {
    Data  = 0,
    Fin   = 1,
    State = 2,
    Reset = 3,
    Syn   = 4,
}

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub enum ExtensionType {
    SelectiveAck = 1,
}

#[derive(Clone)]
pub struct Extension {
    ty: ExtensionType,
    pub data: Vec<u8>,
}

impl Extension {
    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn get_type(&self) -> ExtensionType {
        self.ty
    }

    pub fn iter(&self) -> BitIterator {
        BitIterator::from_bytes(&self.data)
    }
}

#[derive(Clone, Copy)]
struct PacketHeader {
    type_ver: u8, // type: u4, ver: u4
    extension: u8,
    connection_id: u16,
    timestamp_microseconds: u32,
    timestamp_difference_microseconds: u32,
    wnd_size: u32,
    seq_nr: u16,
    ack_nr: u16,
}

impl PacketHeader {
    /// Sets the type of packet to the specified type.
    pub fn set_type(&mut self, t: PacketType) {
        let version = 0x0F & self.type_ver;
        self.type_ver = (t as u8) << 4 | version;
    }

    /// Returns the packet's type.
    pub fn get_type(&self) -> PacketType {
        unsafe { transmute(self.type_ver >> 4) }
    }

    /// Returns the packet's version.
    pub fn get_version(&self) -> u8 {
        self.type_ver & 0x0F
    }

    /// Returns the packet header's length.
    pub fn len(&self) -> usize {
        return HEADER_SIZE;
    }
}

impl Deref for PacketHeader {
    type Target = [u8];

    /// Returns the packet header as a slice of bytes.
    fn deref(&self) -> &[u8] {
        let buf: &[u8; HEADER_SIZE] = unsafe { transmute(self) };
        return &buf[..];
    }
}

impl Decodable for PacketHeader {
    /// Reads a byte buffer and returns the corresponding packet header.
    /// It assumes the fields are in network (big-endian) byte order,
    /// preserving it.
    fn from_bytes(buf: &[u8]) -> Result<PacketHeader, ParseError> {
        // Check length
        if buf.len() < HEADER_SIZE {
            return Err(ParseError::InvalidPacketLength);
        }

        // Check version
        if buf[0] & 0x0F != 1 {
            return Err(ParseError::UnsupportedVersion);
        }

        // Check packet type
        let _ = match buf[0] >> 4 {
            0 => PacketType::Data,
            1 => PacketType::Fin,
            2 => PacketType::State,
            3 => PacketType::Reset,
            4 => PacketType::Syn,
            _ => return Err(ParseError::InvalidPacketType)
        };

        Ok(PacketHeader {
            type_ver: buf[0],
            extension: buf[1],
            connection_id: u8_to_unsigned_be!(buf, 2, 3, u16),
            timestamp_microseconds: u8_to_unsigned_be!(buf, 4, 7, u32),
            timestamp_difference_microseconds: u8_to_unsigned_be!(buf, 8, 11, u32),
            wnd_size: u8_to_unsigned_be!(buf, 12, 15, u32),
            seq_nr: u8_to_unsigned_be!(buf, 16, 17, u16),
            ack_nr: u8_to_unsigned_be!(buf, 18, 19, u16),
        })
    }
}

impl Default for PacketHeader {
    fn default() -> PacketHeader {
        PacketHeader {
            type_ver: (PacketType::Data as u8) << 4 | 1,
            extension: 0,
            connection_id: 0,
            timestamp_microseconds: 0,
            timestamp_difference_microseconds: 0,
            wnd_size: 0,
            seq_nr: 0,
            ack_nr: 0,
        }
    }
}

impl fmt::Debug for PacketHeader {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "(type: {:?}, version: {}, extension: {}, \
                connection_id: {}, timestamp_microseconds: {}, \
                timestamp_difference_microseconds: {}, wnd_size: {}, \
                seq_nr: {}, ack_nr: {})",
                self.get_type(),
                u8::from_be(self.get_version()),
                u8::from_be(self.extension),
                u16::from_be(self.connection_id),
                u32::from_be(self.timestamp_microseconds),
                u32::from_be(self.timestamp_difference_microseconds),
                u32::from_be(self.wnd_size),
                u16::from_be(self.seq_nr),
                u16::from_be(self.ack_nr),
        )
    }
}

pub struct Packet {
    header: PacketHeader,
    pub extensions: Vec<Extension>,
    pub payload: Vec<u8>,
}

impl Packet {
    /// Constructs a new, empty packet.
    pub fn new() -> Packet {
        Packet {
            header: PacketHeader::default(),
            extensions: Vec::new(),
            payload: Vec::new(),
        }
    }

    /// Constructs a new data packet with the given payload.
    pub fn with_payload(payload: &[u8]) -> Packet {
        let mut header = PacketHeader::default();
        header.set_type(PacketType::Data);

        let elts = payload.len();
        let mut v = Vec::with_capacity(elts);

        unsafe {
            use std::ptr;
            v.set_len(elts);
            ptr::copy_nonoverlapping(payload.as_ptr(), v.as_mut_ptr(), elts);
        }

        Packet {
            header: header,
            extensions: Vec::new(),
            payload: v,
        }
    }

    #[inline]
    pub fn set_type(&mut self, t: PacketType) {
        self.header.set_type(t);
    }

    #[inline]
    pub fn get_type(&self) -> PacketType {
        self.header.get_type()
    }

    make_getter!(seq_nr, u16, u16);
    make_getter!(ack_nr, u16, u16);
    make_getter!(connection_id, u16, u16);
    make_getter!(wnd_size, u32, u32);
    make_getter!(timestamp_microseconds, u32, u32);
    make_getter!(timestamp_difference_microseconds, u32, u32);

    make_setter!(set_seq_nr, seq_nr, u16);
    make_setter!(set_ack_nr, ack_nr, u16);
    make_setter!(set_connection_id, connection_id, u16);
    make_setter!(set_wnd_size, wnd_size, u32);
    make_setter!(set_timestamp_microseconds, timestamp_microseconds, u32);
    make_setter!(set_timestamp_difference_microseconds, timestamp_difference_microseconds, u32);

    /// Sets Selective ACK field in packet header and adds appropriate data.
    ///
    /// The length of the SACK extension is expressed in bytes, which
    /// must be a multiple of 4 and at least 4.
    pub fn set_sack(&mut self, bv: Vec<u8>) {
        // The length of the SACK extension is expressed in bytes, which
        // must be a multiple of 4 and at least 4.
        assert!(bv.len() >= 4);
        assert!(bv.len() % 4 == 0);

        let extension = Extension {
            ty: ExtensionType::SelectiveAck,
            data: bv,
        };
        self.extensions.push(extension);
        self.header.extension |= ExtensionType::SelectiveAck as u8;
    }

    pub fn len(&self) -> usize {
        let ext_len = self.extensions.iter().fold(0, |acc, ext| acc + ext.len() + 2);
        self.header.len() + self.payload.len() + ext_len
    }
}

impl Encodable for Packet {
    fn to_bytes(&self) -> Vec<u8> {
        use std::ptr;
        let mut buf: Vec<u8> = Vec::with_capacity(self.len());

        // Copy header
        unsafe {
            ptr::copy(self.header.as_ptr(), buf.as_mut_ptr(), self.header.len());
            buf.set_len(self.header.len());
        }

        // Copy extensions
        let mut extensions = self.extensions.iter().peekable();
        while let Some(extension) = extensions.next() {
            // Extensions are a linked list in which each entry contains:
            // - a byte with the type of the next extension or 0 to end the list,
            // - a byte with the length in bytes of this extension,
            // - the content of this extension.
            buf.push(extensions.peek().map_or(0, |next| next.ty as u8));
            buf.push(extension.len() as u8);
            buf.extend(extension.data.clone());
        }

        // Copy payload
        unsafe {
            let buf_len = buf.len();
            ptr::copy(self.payload.as_ptr(),
                      buf.as_mut_ptr().offset(buf.len() as isize),
                      self.payload.len());
            buf.set_len(buf_len + self.payload.len());
        }

        return buf;
    }
}

impl Decodable for Packet {
    /// Decodes a byte slice and construct the equivalent Packet.
    ///
    /// Note that this method makes no attempt to guess the payload size, saving
    /// all except the initial 20 bytes corresponding to the header as payload.
    /// It's the caller's responsibility to use an appropriately sized buffer.
    fn from_bytes(buf: &[u8]) -> Result<Packet, ParseError> {
        let header = try!(PacketHeader::from_bytes(buf));

        let mut extensions = Vec::new();
        let mut idx = HEADER_SIZE;
        let mut kind = header.extension;

        if buf.len() == HEADER_SIZE && header.extension != 0 {
            return Err(ParseError::InvalidExtensionLength);
        }

        // Consume known extensions and skip over unknown ones
        while idx < buf.len() && kind != 0 {
            if buf.len() < idx + 2 {
                return Err(ParseError::InvalidPacketLength);
            }
            let len = buf[idx + 1] as usize;
            let extension_start = idx + 2;
            let payload_start = extension_start + len;

            // Check validity of extension length:
            // - non-zero,
            // - multiple of 4,
            // - does not exceed packet length
            if len == 0 || len % 4 != 0 || payload_start > buf.len() {
                return Err(ParseError::InvalidExtensionLength);
            }

            let extension = Extension {
                ty: unsafe { transmute(kind) },
                data: buf[extension_start..payload_start].to_vec(),
            };
            extensions.push(extension);

            kind = buf[idx];
            idx += len + 2;
        }
        // Check for pending extensions (early exit of previous loop)
        if kind != 0 {
            return Err(ParseError::InvalidPacketLength);
        }

        let mut payload;
        if idx < buf.len() {
            let payload_length = buf.len() - idx;
            payload = Vec::with_capacity(payload_length);
            unsafe {
                use std::ptr;
                ptr::copy(buf.as_ptr().offset(idx as isize), payload.as_mut_ptr(), payload_length);
                payload.set_len(payload_length);
            }
        } else {
            payload = Vec::new();
        }

        Ok(Packet {
            header: header,
            extensions: extensions,
            payload: payload,
        })
    }
}

impl Clone for Packet {
    fn clone(&self) -> Packet {
        Packet {
            header: self.header,
            extensions: self.extensions.clone(),
            payload: self.payload.clone(),
        }
    }
}

impl fmt::Debug for Packet {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.header.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::PacketType::{State, Data};

    #[test]
    fn test_packet_decode() {
        let buf = [0x21, 0x00, 0x41, 0xa8, 0x99, 0x2f, 0xd0, 0x2a, 0x9f, 0x4a,
                   0x26, 0x21, 0x00, 0x10, 0x00, 0x00, 0x3a, 0xf2, 0x6c, 0x79];
        let pkt = Packet::from_bytes(&buf);
        assert!(pkt.is_ok());
        let pkt = pkt.unwrap();
        assert_eq!(pkt.header.get_version(), 1);
        assert_eq!(pkt.header.get_type(), State);
        assert_eq!(pkt.header.extension, 0);
        assert_eq!(pkt.connection_id(), 16808);
        assert_eq!(pkt.timestamp_microseconds(), 2570047530);
        assert_eq!(pkt.timestamp_difference_microseconds(), 2672436769);
        assert_eq!(pkt.wnd_size(), 2u32.pow(20));
        assert_eq!(pkt.seq_nr(), 15090);
        assert_eq!(pkt.ack_nr(), 27769);
        assert_eq!(pkt.len(), buf.len());
        assert!(pkt.payload.is_empty());
    }

    #[test]
    fn test_decode_packet_with_extension() {
        let buf = [0x21, 0x01, 0x41, 0xa7, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                   0x00, 0x00, 0x00, 0x00, 0x05, 0xdc, 0xab, 0x53, 0x3a, 0xf5,
                   0x00, 0x04, 0x00, 0x00, 0x00, 0x00];
        let packet = Packet::from_bytes(&buf);
        assert!(packet.is_ok());
        let packet = packet.unwrap();
        assert_eq!(packet.header.get_version(), 1);
        assert_eq!(packet.header.get_type(), State);
        assert_eq!(packet.header.extension, 1);
        assert_eq!(packet.connection_id(), 16807);
        assert_eq!(packet.timestamp_microseconds(), 0);
        assert_eq!(packet.timestamp_difference_microseconds(), 0);
        assert_eq!(packet.wnd_size(), 1500);
        assert_eq!(packet.seq_nr(), 43859);
        assert_eq!(packet.ack_nr(), 15093);
        assert_eq!(packet.len(), buf.len());
        assert!(packet.payload.is_empty());
        assert!(packet.extensions.len() == 1);
        assert!(packet.extensions[0].ty == ExtensionType::SelectiveAck);
        assert!(packet.extensions[0].data == vec!(0, 0, 0, 0));
        assert!(packet.extensions[0].len() == packet.extensions[0].data.len());
        assert!(packet.extensions[0].len() == 4);
        // Reversible
        assert_eq!(packet.to_bytes(), &buf);
    }

    #[test]
    fn test_packet_decode_with_missing_extension() {
        let buf = [0x21, 0x01, 0x41, 0xa8, 0x99, 0x2f, 0xd0, 0x2a, 0x9f, 0x4a,
                   0x26, 0x21, 0x00, 0x10, 0x00, 0x00, 0x3a, 0xf2, 0x6c, 0x79];
        let pkt = Packet::from_bytes(&buf);
        assert!(pkt.is_err());
    }

    #[test]
    fn test_packet_decode_with_malformed_extension() {
        let buf = [0x21, 0x01, 0x41, 0xa8, 0x99, 0x2f, 0xd0, 0x2a, 0x9f, 0x4a,
                   0x26, 0x21, 0x00, 0x10, 0x00, 0x00, 0x3a, 0xf2, 0x6c, 0x79,
                   0x00, 0x04, 0x00];
        let pkt = Packet::from_bytes(&buf);
        assert!(pkt.is_err());
    }

    #[test]
    fn test_decode_packet_with_unknown_extensions() {
        let buf = [0x21, 0x01, 0x41, 0xa7, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                   0x00, 0x00, 0x00, 0x00, 0x05, 0xdc, 0xab, 0x53, 0x3a, 0xf5,
                   0xff, 0x04, 0x00, 0x00, 0x00, 0x00, // Imaginary extension
                   0x00, 0x04, 0x00, 0x00, 0x00, 0x00];
        let packet = Packet::from_bytes(&buf);
        assert!(packet.is_ok());
        let packet = packet.unwrap();
        assert_eq!(packet.header.get_version(), 1);
        assert_eq!(packet.header.get_type(), State);
        assert_eq!(packet.header.extension, 1);
        assert_eq!(packet.connection_id(), 16807);
        assert_eq!(packet.timestamp_microseconds(), 0);
        assert_eq!(packet.timestamp_difference_microseconds(), 0);
        assert_eq!(packet.wnd_size(), 1500);
        assert_eq!(packet.seq_nr(), 43859);
        assert_eq!(packet.ack_nr(), 15093);
        assert!(packet.payload.is_empty());
        assert!(packet.extensions.len() == 2);
        assert!(packet.extensions[0].ty == ExtensionType::SelectiveAck);
        assert!(packet.extensions[0].data == vec!(0, 0, 0, 0));
        assert!(packet.extensions[0].len() == packet.extensions[0].data.len());
        assert!(packet.extensions[0].len() == 4);
        // Reversible
        assert_eq!(packet.to_bytes(), &buf);
    }

    #[test]
    fn test_packet_encode() {
        let payload = "Hello\n".as_bytes().to_vec();
        let (timestamp, timestamp_diff): (u32, u32) = (15270793, 1707040186);
        let (connection_id, seq_nr, ack_nr): (u16, u16, u16) = (16808, 15090, 17096);
        let window_size: u32 = 1048576;
        let mut pkt = Packet::new();
        pkt.set_type(Data);
        pkt.header.timestamp_microseconds = timestamp.to_be();
        pkt.header.timestamp_difference_microseconds = timestamp_diff.to_be();
        pkt.header.connection_id = connection_id.to_be();
        pkt.header.seq_nr = seq_nr.to_be();
        pkt.header.ack_nr = ack_nr.to_be();
        pkt.header.wnd_size = window_size.to_be();
        pkt.payload = payload.clone();
        let header = pkt.header;
        let buf = [0x01, 0x00, 0x41, 0xa8, 0x00, 0xe9, 0x03, 0x89,
                   0x65, 0xbf, 0x5d, 0xba, 0x00, 0x10, 0x00, 0x00,
                   0x3a, 0xf2, 0x42, 0xc8, 0x48, 0x65, 0x6c, 0x6c,
                   0x6f, 0x0a];

        assert_eq!(pkt.len(), buf.len());
        assert_eq!(pkt.len(), HEADER_SIZE + payload.len());
        assert_eq!(pkt.payload, payload);
        assert_eq!(header.get_version(), 1);
        assert_eq!(header.get_type(), Data);
        assert_eq!(header.extension, 0);
        assert_eq!(pkt.connection_id(), connection_id);
        assert_eq!(pkt.seq_nr(), seq_nr);
        assert_eq!(pkt.ack_nr(), ack_nr);
        assert_eq!(pkt.wnd_size(), window_size);
        assert_eq!(pkt.timestamp_microseconds(), timestamp);
        assert_eq!(pkt.timestamp_difference_microseconds(), timestamp_diff);
        assert_eq!(pkt.to_bytes(), buf.to_vec());
    }

    #[test]
    fn test_packet_encode_with_payload() {
        let payload = "Hello\n".as_bytes().to_vec();
        let (timestamp, timestamp_diff): (u32, u32) = (15270793, 1707040186);
        let (connection_id, seq_nr, ack_nr): (u16, u16, u16) = (16808, 15090, 17096);
        let window_size: u32 = 1048576;
        let mut pkt = Packet::with_payload(&payload[..]);
        pkt.header.timestamp_microseconds = timestamp.to_be();
        pkt.header.timestamp_difference_microseconds = timestamp_diff.to_be();
        pkt.header.connection_id = connection_id.to_be();
        pkt.header.seq_nr = seq_nr.to_be();
        pkt.header.ack_nr = ack_nr.to_be();
        pkt.header.wnd_size = window_size.to_be();
        pkt.payload = payload.clone();
        let header = pkt.header;
        let buf = [0x01, 0x00, 0x41, 0xa8, 0x00, 0xe9, 0x03, 0x89,
                   0x65, 0xbf, 0x5d, 0xba, 0x00, 0x10, 0x00, 0x00,
                   0x3a, 0xf2, 0x42, 0xc8, 0x48, 0x65, 0x6c, 0x6c,
                   0x6f, 0x0a];

        assert_eq!(pkt.len(), buf.len());
        assert_eq!(pkt.len(), HEADER_SIZE + payload.len());
        assert_eq!(pkt.payload, payload);
        assert_eq!(header.get_version(), 1);
        assert_eq!(header.get_type(), Data);
        assert_eq!(header.extension, 0);
        assert_eq!(pkt.connection_id(), connection_id);
        assert_eq!(pkt.seq_nr(), seq_nr);
        assert_eq!(pkt.ack_nr(), ack_nr);
        assert_eq!(pkt.wnd_size(), window_size);
        assert_eq!(pkt.timestamp_microseconds(), timestamp);
        assert_eq!(pkt.timestamp_difference_microseconds(), timestamp_diff);
        assert_eq!(pkt.to_bytes(), buf.to_vec());
    }

    #[test]
    fn test_packet_encode_with_multiple_extensions() {
        let mut packet = Packet::new();
        let extension = Extension { ty: ExtensionType::SelectiveAck, data: vec!(1, 2, 3, 4) };
        packet.header.extension = extension.ty as u8;
        packet.extensions.push(extension.clone());
        packet.extensions.push(extension.clone());
        let bytes = packet.to_bytes();
        assert_eq!(bytes.len(), HEADER_SIZE + (extension.len() + 2) * 2);

        // Type of the first extension
        assert_eq!(bytes[1], extension.ty as u8);

        // Type of the next (second) extension
        assert_eq!(bytes[HEADER_SIZE], extension.ty as u8);
        // Length of the first extension
        assert_eq!(bytes[HEADER_SIZE + 1], extension.data.len() as u8);

        // Type of the next (third, non-existent) extension
        assert_eq!(bytes[HEADER_SIZE + 2 + extension.len()], 0);
        // Length of the second extension
        assert_eq!(bytes[HEADER_SIZE + 2 + extension.len() + 1], extension.data.len() as u8);
    }

    #[test]
    fn test_reversible() {
        let buf = [0x01, 0x00, 0x41, 0xa8, 0x00, 0xe9, 0x03, 0x89,
                   0x65, 0xbf, 0x5d, 0xba, 0x00, 0x10, 0x00, 0x00,
                   0x3a, 0xf2, 0x42, 0xc8, 0x48, 0x65, 0x6c, 0x6c,
                   0x6f, 0x0a];
        assert_eq!(&Packet::from_bytes(&buf).unwrap().to_bytes()[..], &buf[..]);
    }

    #[test]
    fn test_decode_evil_sequence() {
        let buf = [0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let packet = Packet::from_bytes(&buf);
        assert!(packet.is_err());
    }

    #[test]
    fn test_decode_empty_packet() {
        let packet = Packet::from_bytes(&[]);
        assert!(packet.is_err());
    }

    // Use quickcheck to simulate a malicious attacker sending malformed packets
    #[test]
    fn quicktest() {
        use quickcheck::{QuickCheck, TestResult};

        fn run(x: Vec<u8>) -> TestResult {
            let packet = Packet::from_bytes(&x[..]);

            if x.len() < 20 {
                // Header too small
                TestResult::from_bool(packet.is_err())
            } else if x[0] & 0x0F != 1 {
                // Invalid version
                TestResult::from_bool(packet.is_err())
            } else if (x[0] >> 4) > 4 {
                // Invalid packet type
                TestResult::from_bool(packet.is_err())
            } else if x[1] != 0 {
                // Non-empty extension field, check validity of extension(s)
                if x.len() < HEADER_SIZE + 2 {
                    return TestResult::from_bool(packet.is_err());
                }

                let mut next_kind = x[1];
                let mut idx = HEADER_SIZE;

                while idx < x.len() && next_kind != 0 {
                    if x.len() < idx + 2 {
                        return TestResult::from_bool(packet.is_err());
                    }
                    let len = x[idx + 1] as usize;
                    next_kind = x[idx];

                    // Check validity of extension length:
                    // - non-zero,
                    // - multiple of 4,
                    // - does not exceed packet length
                    if len == 0 || len % 4 != 0 || x.len() < idx + len + 2 {
                        return TestResult::from_bool(packet.is_err());
                    }

                    idx += len + 2;
                }
                TestResult::from_bool(packet.is_ok() || next_kind != 0)
            } else {
                TestResult::from_bool(packet.is_ok() && packet.unwrap().to_bytes() == x)
            }
        }
        QuickCheck::new().tests(10000).quickcheck(run as fn(Vec<u8>) -> TestResult)
    }
}
