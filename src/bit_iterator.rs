/// Lazy iterator over bits of a vector of bytes, starting with the LSB
/// (least-significat bit) of the first element of the vector.
pub struct BitIterator<'a> {
    object: &'a Vec<u8>,
    current_bit: usize
}

impl<'a> BitIterator<'a> {
    pub fn new(obj: &'a Vec<u8>) -> BitIterator {
        BitIterator { object: obj, current_bit: 0 }
    }
}

impl<'a> Iterator for BitIterator<'a> {
    type Item = u8;

    fn next(&mut self) -> Option<u8> {
        const U8BITS: usize = 8;
        let current_byte = self.current_bit / U8BITS;

        if current_byte == self.object.len() {
            return None;
        } else {
            let result = self.object[current_byte] >> (self.current_bit % U8BITS) & 0x1;
            self.current_bit += 1;
            return Some(result);
        }
    }
}

#[test]
fn test_iterator() {
    let bytes = vec!(0xCA, 0xFE);
    let expected_bits = vec!(0,1,0,1, 0,0,1,1, 0,1,1,1, 1,1,1,1);
    let mut i = 0;

    for bit in BitIterator::new(&bytes) {
        println!("{} == {}", bit, expected_bits[i]);
        assert_eq!(bit, expected_bits[i]);
        i += 1;
    }
}
