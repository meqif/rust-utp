// How many bits in a `u8`
const U8BITS: usize = 8;

/// Lazy iterator over bits of a vector of bytes, starting with the LSB
/// (least-significat bit) of the first element of the vector.
pub struct BitIterator<'a> {
    object: &'a [u8],
    next_idx: usize,
    end_idx: usize,
}

impl<'a> BitIterator<'a> {
    /// Creates an iterator from a vector of bytes. Each byte becomes eight bits, with the least
    /// significant bits coming first.
    pub fn from_bytes(obj: &'a [u8]) -> BitIterator {
        BitIterator { object: obj, next_idx: 0, end_idx: obj.len() * U8BITS }
    }

    /// Returns the number of ones in the binary representation of the underlying object.
    pub fn count_ones(&self) -> u32 {
        self.object.iter().fold(0, |acc, bv| acc + bv.count_ones())
    }
}

impl<'a> Iterator for BitIterator<'a> {
    type Item = bool;

    fn next(&mut self) -> Option<bool> {
        if self.next_idx != self.end_idx {
            let (byte_idx, bit_idx) = (self.next_idx / U8BITS, self.next_idx % U8BITS);
            let bit = self.object[byte_idx] >> bit_idx & 0x1;
            self.next_idx += 1;
            Some(bit == 0x1)
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.end_idx, Some(self.end_idx))
    }
}

impl<'a> ExactSizeIterator for BitIterator<'a> {}

#[test]
fn test_iterator() {
    let bytes = vec!(0xCA, 0xFE);
    let expected_bits = vec!(0, 1, 0, 1, 0, 0, 1, 1, 0, 1, 1, 1, 1, 1, 1, 1);

    for (i, bit) in BitIterator::from_bytes(&bytes).enumerate() {
        println!("{} == {}", bit, expected_bits[i] == 1);
        assert_eq!(bit, expected_bits[i] == 1);
    }
}
