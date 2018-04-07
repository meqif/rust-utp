use bit_iterator::BitIterator;

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub enum ExtensionType {
    None,
    SelectiveAck,
    Unknown(u8),
}

impl From<u8> for ExtensionType {
    fn from(original: u8) -> Self {
        match original {
            0 => ExtensionType::None,
            1 => ExtensionType::SelectiveAck,
            n => ExtensionType::Unknown(n),
        }
    }
}

impl From<ExtensionType> for u8 {
    fn from(original: ExtensionType) -> u8 {
        match original {
            ExtensionType::None => 0,
            ExtensionType::SelectiveAck => 1,
            ExtensionType::Unknown(n) => n,
        }
    }
}

#[derive(Clone)]
pub struct Extension<'a> {
    ty: ExtensionType,
    pub data: &'a [u8],
}

impl<'a> Extension<'a> {
    pub fn new(ty: ExtensionType, data: &'a [u8]) -> Extension<'a> {
        Extension { ty, data }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn get_type(&self) -> ExtensionType {
        self.ty
    }

    pub fn iter(&self) -> BitIterator {
        BitIterator::from_bytes(self.data)
    }
}
