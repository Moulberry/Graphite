use std::sync::Arc;

use once_cell::sync::Lazy;

use super::*;

#[derive(Clone)]
pub struct EncodedNBT {
    bytes: Arc<[u8]>
}

impl EncodedNBT {
    pub fn new_from_raw_bytes(bytes: Vec<u8>) -> Self {
        Self {
            bytes: bytes.into()
        }
    }

    pub fn to_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn ptr_eq(&self, other: &EncodedNBT) -> bool {
        Arc::ptr_eq(&self.bytes, &other.bytes)
    }
}

static EMPTY: Lazy<EncodedNBT> = Lazy::new(|| {
    NBT::new().into()
});

impl Default for EncodedNBT {
    fn default() -> Self {
        EMPTY.clone()
    }
}

impl From<NBT> for EncodedNBT {
    fn from(nbt: NBT) -> Self {
        Self {
            bytes: encode::write_protocol(&nbt).into()
        }
    }
}

impl PartialEq for EncodedNBT {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.bytes, &other.bytes) || self.bytes == other.bytes
    }
}

impl Debug for EncodedNBT {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut bytes = self.to_bytes();
        if let Ok(nbt) = decode::read_protocol(&mut bytes) {
            nbt.fmt(f)
        } else {
            f.write_str("<INVALID NBT>")
        }
    }
}
