use std::{cell::{Ref, RefCell}, ops::{Deref, DerefMut}};

use super::*;

#[derive(Default, Clone)]
pub struct CachedNBT {
    nbt: NBT,
    cached_bytes: RefCell<Vec<u8>>,
}

#[derive(Clone)]
struct CachedNBTInner {
    nbt: NBT,
    bytes: Vec<u8>,
}

impl Debug for CachedNBT {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.nbt.fmt(f)
    }
}

impl From<NBT> for CachedNBT {
    fn from(nbt: NBT) -> Self {
        Self {
            nbt,
            cached_bytes: RefCell::new(Vec::new()),
        }
    }
}

impl PartialEq for CachedNBT {
    fn eq(&self, other: &Self) -> bool {
        self.deref() == other.deref()
    }
}

impl Deref for CachedNBT {
    type Target = NBT;

    fn deref(&self) -> &Self::Target {
        &self.nbt
    }
}

impl DerefMut for CachedNBT {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.cached_bytes.get_mut().clear();
        &mut self.nbt
    }
}

impl CachedNBT {
    pub const fn new() -> Self {
        Self {
            nbt: NBT::new(),
            cached_bytes: RefCell::new(Vec::new()),
        }
    }

    pub fn to_bytes(&self) -> Ref<[u8]> {
        let slice = Ref::map(self.cached_bytes.borrow(), |v| v.as_slice());

        if slice.is_empty() {
            drop(slice);

            let mut cached_bytes = self.cached_bytes.borrow_mut();
            encode::write_protocol_into(&self.nbt, &mut cached_bytes);
            drop(cached_bytes);

            Ref::map(self.cached_bytes.borrow(), |v| v.as_slice())
        } else {
            slice
        }
    }
}
