use extension::{Extension, ExtensionType};
use packet::Packet;

pub struct ExtensionIterator<'a> {
    raw_bytes: &'a [u8],
    next_extension: ExtensionType,
    index: usize,
}

impl<'a> ExtensionIterator<'a> {
    pub fn new(packet: &'a Packet) -> Self {
        ExtensionIterator {
            raw_bytes: packet.as_ref(),
            next_extension: ExtensionType::from(packet.as_ref()[1]),
            index: Packet::HEADER_SIZE,
        }
    }
}

impl<'a> Iterator for ExtensionIterator<'a> {
    type Item = Extension<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next_extension == ExtensionType::None {
            None
        } else if self.index < self.raw_bytes.len() {
            let len = self.raw_bytes[self.index + 1] as usize;
            let extension_start = self.index + 2;
            let extension_end = extension_start + len;

            // Assume extension is valid because the bytes come from a (valid) Packet
            let extension = Extension::new(
                self.next_extension,
                &self.raw_bytes[extension_start..extension_end],
            );

            self.next_extension = self.raw_bytes[self.index].into();
            self.index += len + 2;

            Some(extension)
        } else {
            None
        }
    }
}
