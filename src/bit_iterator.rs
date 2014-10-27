/// Lazy iterator over bits of a vector of bytes, starting with the LSB
/// (least-significat bit) of the first element of the vector.
pub struct BitIterator { object: Vec<u8>, current_byte: uint, current_bit: uint }

impl BitIterator {
    pub fn new(obj: Vec<u8>) -> BitIterator {
        BitIterator { object: obj, current_byte: 0, current_bit: 0 }
    }
}

impl Iterator<u8> for BitIterator {
    fn next(&mut self) -> Option<u8> {
        let result = self.object[self.current_byte] >> self.current_bit & 0x1;

        if self.current_bit + 1 == ::std::u8::BITS {
            self.current_byte += 1;
        }
        self.current_bit = (self.current_bit + 1) % ::std::u8::BITS;

        if self.current_byte == self.object.len() {
            return None;
        } else {
            return Some(result);
        }
    }
}
