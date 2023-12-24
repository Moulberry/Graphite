use std::{
    cell::RefCell,
    ops::{Deref, DerefMut},
};

use super::*;

#[derive(Clone)]
pub struct CachedNBT {
    inner: RefCell<CachedNBTInner>,
}

#[derive(Clone)]
struct CachedNBTInner {
    nbt: NBT,
    bytes_dirty: bool,
    bytes: Vec<u8>,
}

impl Debug for CachedNBT {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.inner.borrow().nbt.fmt(f)
    }
}

impl From<NBT> for CachedNBT {
    fn from(nbt: NBT) -> Self {
        Self {
            inner: RefCell::new(CachedNBTInner {
                nbt,
                bytes_dirty: true,
                bytes: Vec::new(),
            }),
        }
    }
}

impl Deref for CachedNBT {
    type Target = NBT;

    fn deref(&self) -> &Self::Target {
        &unsafe { &*self.inner.as_ptr() }.nbt
    }
}

impl DerefMut for CachedNBT {
    fn deref_mut(&mut self) -> &mut Self::Target {
        let inner = self.inner.get_mut();
        inner.bytes_dirty = true;
        &mut inner.nbt
    }
}

impl CachedNBT {
    pub fn new() -> Self {
        Self {
            inner: RefCell::new(CachedNBTInner {
                nbt: NBT::new(),
                bytes_dirty: true,
                bytes: Vec::new(),
            }),
        }
    }

    pub fn to_bytes(&self) -> &[u8] {
        if self.inner.borrow().bytes_dirty {
            let inner = unsafe { &mut *self.inner.as_ptr() };
            inner.bytes_dirty = false;
            inner.bytes.truncate(0);

            encode::write_into(&inner.nbt, &mut inner.bytes);
        }

        // &[0]
        unsafe { &*self.inner.as_ptr() }.bytes.as_slice()
    }
}
