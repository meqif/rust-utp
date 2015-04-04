use std::mem::transmute;
use std::fmt;
use std::num::Int;
use bit_iterator::BitIterator;

pub const HEADER_SIZE: usize = 20;

macro_rules! u8_to_unsigned_be {
    ($src:ident, $start:expr, $end:expr, $t:ty) => ({
        let mut result: $t = 0;
        for i in (0usize .. $end - $start + 1).rev() {
            result = result | $src[$start+i] as $t << i*8;
        }
        result
    })
}

#[derive(PartialEq,Eq,Debug)]
pub enum PacketType {
    Data  = 0,
    Fin   = 1,
    State = 2,
    Reset = 3,
    Syn   = 4,
}

#[derive(PartialEq,Eq,Debug,Clone,Copy)]
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
        1 + self.data.len()
    }

    pub fn get_type(&self) -> ExtensionType {
        self.ty
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut data = vec!(self.data.len() as u8);
        data.extend(self.data.iter().map(|&x| x));
        return data;
    }

    pub fn iter(&self) -> BitIterator {
        BitIterator::new(&self.data)
    }
}

#[derive(Clone,Copy)]
#[packed]
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
    /// Set type of packet to the specified type.
    pub fn set_type(&mut self, t: PacketType) {
        let version = 0x0F & self.type_ver;
        self.type_ver = (t as u8) << 4 | version;
    }

    pub fn get_type(&self) -> PacketType {
        unsafe { transmute(self.type_ver >> 4) }
    }

    pub fn get_version(&self) -> u8 {
        self.type_ver & 0x0F
    }

    /// Return packet header as a slice of bytes.
    pub fn bytes(&self) -> &[u8] {
        let buf: &[u8; HEADER_SIZE] = unsafe { transmute(self) };
        return &buf[..];
    }

    pub fn len(&self) -> usize {
        return HEADER_SIZE;
    }

    /// Read byte buffer and return corresponding packet header.
    /// It assumes the fields are in network (big-endian) byte order,
    /// preserving it.
    pub fn decode(buf: &[u8]) -> PacketHeader {
        PacketHeader {
            type_ver: buf[0],
            extension: buf[1],
            connection_id: u8_to_unsigned_be!(buf, 2, 3, u16),
            timestamp_microseconds: u8_to_unsigned_be!(buf, 4, 7, u32),
            timestamp_difference_microseconds: u8_to_unsigned_be!(buf, 8, 11, u32),
            wnd_size: u8_to_unsigned_be!(buf, 12, 15, u32),
            seq_nr: u8_to_unsigned_be!(buf, 16, 17, u16),
            ack_nr: u8_to_unsigned_be!(buf, 18, 19, u16),
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

pub struct Packet {
    header: PacketHeader,
    pub extensions: Vec<Extension>,
    pub payload: Vec<u8>,
}

impl Packet {
    /// Construct a new, empty packet.
    pub fn new() -> Packet {
        Packet {
            header: PacketHeader {
                type_ver: (PacketType::Data as u8) << 4 | 1,
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
    pub fn set_type(&mut self, t: PacketType) {
        self.header.set_type(t);
    }

    #[inline]
    pub fn get_type(&self) -> PacketType {
        self.header.get_type()
    }

    #[inline]
    pub fn seq_nr(&self) -> u16 {
        Int::from_be(self.header.seq_nr)
    }

    #[inline]
    pub fn set_seq_nr(&mut self, seq_nr: u16) {
        self.header.seq_nr = seq_nr.to_be();
    }

    #[inline]
    pub fn ack_nr(&self) -> u16 {
        Int::from_be(self.header.ack_nr)
    }

    #[inline]
    pub fn set_ack_nr(&mut self, ack_nr: u16) {
        self.header.ack_nr = ack_nr.to_be()
    }

    #[inline]
    pub fn connection_id(&self) -> u16 {
        Int::from_be(self.header.connection_id)
    }

    #[inline]
    pub fn set_connection_id(&mut self, conn_id: u16) {
        self.header.connection_id = conn_id.to_be();
    }

    #[inline]
    pub fn set_wnd_size(&mut self, new_wnd_size: u32) {
        self.header.wnd_size = new_wnd_size.to_be();
    }

    #[inline]
    pub fn wnd_size(&self) -> u32 {
        Int::from_be(self.header.wnd_size)
    }

    #[inline]
    pub fn timestamp_microseconds(&self) -> u32 {
        Int::from_be(self.header.timestamp_microseconds)
    }

    #[inline]
    pub fn set_timestamp_microseconds(&mut self, tstamp: u32) {
        self.header.timestamp_microseconds = tstamp.to_be();
    }

    #[inline]
    pub fn timestamp_difference_microseconds(&self) -> u32 {
        Int::from_be(self.header.timestamp_difference_microseconds)
    }

    #[inline]
    pub fn set_timestamp_difference_microseconds(&mut self, tstamp: u32) {
        self.header.timestamp_difference_microseconds = tstamp.to_be();
    }

    /// Set Selective ACK field in packet header and add appropriate data.
    ///
    /// If None is passed, the SACK extension is disabled and the respective
    /// data is flushed. Otherwise, the SACK extension is enabled and the
    /// vector `v` is taken as the extension's payload.
    ///
    /// The length of the SACK extension is expressed in bytes, which
    /// must be a multiple of 4 and at least 4.
    pub fn set_sack(&mut self, v: Option<Vec<u8>>) {
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

                let extension = Extension {
                    ty: ExtensionType::SelectiveAck,
                    data: bv,
                };
                self.extensions.push(extension);
                self.header.extension |= ExtensionType::SelectiveAck as u8;
            }
        }
    }

    pub fn bytes(&self) -> Vec<u8> {
        let mut buf: Vec<u8> = Vec::with_capacity(self.len());
        buf.extend(self.header.bytes().iter().map(|a| *a));

        let mut extensions = self.extensions.iter().peekable();
        while let Some(extension) = extensions.next() {
            // next extension id
            match extensions.peek() {
                None => buf.push(0u8),
                Some(next) => buf.push(next.ty as u8),
            }
            buf.extend(extension.to_bytes());
        }

        buf.extend(self.payload.clone());
        return buf;
    }

    pub fn len(&self) -> usize {
        let ext_len = self.extensions.iter().fold(0, |acc, ext| acc + ext.len() + 1);
        self.header.len() + self.payload.len() + ext_len
    }

    /// Decode a byte slice and construct the equivalent Packet.
    ///
    /// Note that this method makes no attempt to guess the payload size, saving
    /// all except the initial 20 bytes corresponding to the header as payload.
    /// It's the caller's responsability to use an appropriately sized buffer.
    pub fn decode(buf: &[u8]) -> Packet {
        let header = PacketHeader::decode(buf);

        let mut extensions = Vec::new();
        let mut idx = HEADER_SIZE;
        let mut kind = header.extension;

        // Consume known extensions and skip over unknown ones
        while idx < buf.len() && kind != 0 {
            let len = buf[idx + 1] as usize;
            let extension_start = idx + 2;
            let payload_start = extension_start + len;

            if kind == ExtensionType::SelectiveAck as u8 { // or more generally, a known kind
                let extension = Extension {
                    ty: ExtensionType::SelectiveAck,
                    data: buf[extension_start..payload_start].to_vec(),
                };
                extensions.push(extension);
            }

            kind = buf[idx];
            idx += payload_start;
        }

        let mut payload;
        if idx < buf.len() {
            payload = buf[idx..].to_vec();
        } else {
            payload = Vec::new();
        }

        Packet {
            header: header,
            extensions: extensions,
            payload: payload,
        }
    }

    /// Return a clone of this object without the payload
    pub fn shallow_clone(&self) -> Packet {
        Packet {
            header: self.header.clone(),
            extensions: self.extensions.clone(),
            payload: Vec::new(),
        }
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
mod test {
    use super::Packet;
    use super::PacketType::{State, Data};
    use super::ExtensionType;
    use super::HEADER_SIZE;
    use std::num::Int;

    #[test]
    fn test_packet_decode() {
        let buf = [0x21, 0x00, 0x41, 0xa8, 0x99, 0x2f, 0xd0, 0x2a, 0x9f, 0x4a,
                   0x26, 0x21, 0x00, 0x10, 0x00, 0x00, 0x3a, 0xf2, 0x6c, 0x79];
        let pkt = Packet::decode(&buf);
        assert_eq!(pkt.header.get_version(), 1);
        assert_eq!(pkt.header.get_type(), State);
        assert_eq!(pkt.header.extension, 0);
        assert_eq!(pkt.connection_id(), 16808);
        assert_eq!(Int::from_be(pkt.header.timestamp_microseconds), 2570047530);
        assert_eq!(Int::from_be(pkt.header.timestamp_difference_microseconds), 2672436769);
        assert_eq!(Int::from_be(pkt.header.wnd_size), 2u32.pow(20));
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
        let packet = Packet::decode(&buf);
        assert_eq!(packet.header.get_version(), 1);
        assert_eq!(packet.header.get_type(), State);
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
        assert!(packet.extensions[0].ty == ExtensionType::SelectiveAck);
        assert!(packet.extensions[0].data == vec!(0,0,0,0));
        assert!(packet.extensions[0].len() == 1 + packet.extensions[0].data.len());
        assert!(packet.extensions[0].len() == 5);
    }

    #[test]
    fn test_decode_packet_with_unknown_extensions() {
        let buf = [0x21, 0x01, 0x41, 0xa7, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                   0x00, 0x00, 0x00, 0x00, 0x05, 0xdc, 0xab, 0x53, 0x3a, 0xf5,
                   0xff, 0x04, 0x00, 0x00, 0x00, 0x00, // Imaginary extension
                   0x00, 0x04, 0x00, 0x00, 0x00, 0x00];
        let packet = Packet::decode(&buf);
        assert_eq!(packet.header.get_version(), 1);
        assert_eq!(packet.header.get_type(), State);
        assert_eq!(packet.header.extension, 1);
        assert_eq!(packet.connection_id(), 16807);
        assert_eq!(Int::from_be(packet.header.timestamp_microseconds), 0);
        assert_eq!(Int::from_be(packet.header.timestamp_difference_microseconds), 0);
        assert_eq!(Int::from_be(packet.header.wnd_size), 1500);
        assert_eq!(packet.seq_nr(), 43859);
        assert_eq!(packet.ack_nr(), 15093);
        assert!(packet.payload.is_empty());
        assert!(packet.extensions.len() == 1);
        assert!(packet.extensions[0].ty == ExtensionType::SelectiveAck);
        assert!(packet.extensions[0].data == vec!(0,0,0,0));
        assert!(packet.extensions[0].len() == 1 + packet.extensions[0].data.len());
        assert!(packet.extensions[0].len() == 5);
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
        assert_eq!(Int::from_be(header.connection_id), connection_id);
        assert_eq!(Int::from_be(header.seq_nr), seq_nr);
        assert_eq!(Int::from_be(header.ack_nr), ack_nr);
        assert_eq!(Int::from_be(header.wnd_size), window_size);
        assert_eq!(Int::from_be(header.timestamp_microseconds), timestamp);
        assert_eq!(Int::from_be(header.timestamp_difference_microseconds), timestamp_diff);
        assert_eq!(pkt.bytes(), buf.to_vec());
    }

    #[test]
    fn test_reversible() {
        let buf = [0x01, 0x00, 0x41, 0xa8, 0x00, 0xe9, 0x03, 0x89,
                   0x65, 0xbf, 0x5d, 0xba, 0x00, 0x10, 0x00, 0x00,
                   0x3a, 0xf2, 0x42, 0xc8, 0x48, 0x65, 0x6c, 0x6c,
                   0x6f, 0x0a];
        assert_eq!(&Packet::decode(&buf).bytes()[..], &buf[..]);
    }

}
