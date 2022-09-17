use std::{iter::Enumerate, slice::{Iter, IterMut}};

use crate::Unsticky;

#[derive(Debug)]
pub struct StickyVec<T: Unsticky> {
    inner: Vec<T>
}

impl<T: Unsticky> Default for StickyVec<T> {
    fn default() -> Self {
        Self { inner: Default::default() }
    }
}

impl<T: Unsticky> StickyVec<T> {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn push(&mut self, value: T) {
        let before_capacity = self.inner.capacity();
        self.inner.push(value);
        if before_capacity != self.inner.capacity() {
            // Reallocation occured, update pointer of all children
            for child_ref in &mut self.inner {
                child_ref.update_pointer();
            }
        } else {
            // No reallocation occured, just update pointer of the new element
            let last_index = self.inner.len() - 1;
            self.inner[last_index].update_pointer();
        }
    }

    pub fn get(&self, index: usize) -> Option<&T> {
        self.inner.get(index)
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        self.inner.get_mut(index)
    }

    pub fn swap_remove(&mut self, index: usize) -> T::UnstuckType {
        let removed = self.inner.swap_remove(index).unstick();

        if index < self.inner.len() {
            self.inner[index].update_pointer();
        }

        removed
    }

    pub fn retain<F>(&mut self, mut f: F)
    where
        F: FnMut(&T) -> bool,
    {
        self.retain_mut(|elem| f(elem));
    }

    pub fn retain_mut<F>(&mut self, f: F)
    where
        F: FnMut(&mut T) -> bool,
    {
        let len = self.inner.len();
        let guard = RetainGuard {
            f,
            vec: &mut self.inner,
            index: 0,
            len,
            should_swap: true,
        };
        guard.retain();
    }

    pub fn drain_filter<'a, F>(&'a mut self, f: F) -> DrainFilter<'a, T, F>
    where
        F: FnMut(&mut T) -> bool,
    {
        let len = self.inner.len();
        DrainFilter { f, vec: &mut self.inner, forwards: true, index: 0, len }
    }

    pub fn iter(&self) -> Iter<T> {
        self.inner.iter()
    }

    pub fn iter_mut(&mut self) -> IterMut<T> {
        self.inner.iter_mut()
    }
}

struct RetainGuard<'a, T, F>
where
    T: Unsticky,
    F: FnMut(&mut T) -> bool
{
    f: F,
    vec: &'a mut Vec<T>,
    should_swap: bool,
    index: usize,
    len: usize
}

impl<'a, T, F> Drop for RetainGuard<'a, T, F>
where
    T: Unsticky,
    F: FnMut(&mut T) -> bool
{
    fn drop(&mut self) {
        if self.len > 0 && self.index < self.len - 1 && self.should_swap {
            unsafe {
                std::ptr::copy_nonoverlapping(
                    self.vec.as_mut_ptr().add(self.len - 1), 
                    self.vec.as_mut_ptr().add(self.index), 
                    1
                );   
            }
            self.len -= 1;
        }
        unsafe { self.vec.set_len(self.len); }
    }
}

impl<'a, T, F> RetainGuard<'a, T, F>
where
    T: Unsticky,
    F: FnMut(&mut T) -> bool
{
    fn retain(mut self) {
        while self.index < self.len {
            let item_ptr = unsafe { self.vec.as_mut_ptr().add(self.index) };

            // Check if item should be removed
            if !(self.f)(unsafe { &mut *item_ptr }) {
                // Drop it
                debug_assert!(self.should_swap);
                unsafe { std::ptr::drop_in_place(item_ptr); }
                self.len -= 1;

                // Find suitable element for swap
                while self.len > self.index {
                    let end_item_ptr = unsafe { self.vec.as_mut_ptr().add(self.len) };

                    // Check if end item should also be removed
                    if !(self.f)(unsafe { &mut *end_item_ptr}) {
                        // Drop end item, continue trying to find suitable element
                        debug_assert!(self.should_swap);
                        unsafe { std::ptr::drop_in_place(end_item_ptr); }
                        self.len -= 1;
                        continue;
                    } else {
                        // Found suitable candidate for swap
                        unsafe {
                            // Copy the candidate into the item's position
                            std::ptr::copy_nonoverlapping(end_item_ptr, item_ptr, 1);
                            // Update the pointer of the candidate, in the position of the previous item
                            self.should_swap = false;
                            (&mut *item_ptr).update_pointer();
                            self.should_swap = true;
                        }
                        break;
                    }
                }
            }

            self.index += 1;
        }
    }
}

pub struct DrainFilter<'a, T, F>
where
    T: Unsticky,
    F: FnMut(&mut T) -> bool
{
    f: F,
    vec: &'a mut Vec<T>,
    forwards: bool,
    index: usize,
    len: usize
}

impl<'a, T, F> Drop for DrainFilter<'a, T, F>
where
    T: Unsticky,
    F: FnMut(&mut T) -> bool
{
    fn drop(&mut self) {
        if !self.forwards {
            while self.index < self.len {
                let item = &mut self.vec[self.len];

                if (self.f)(item) {
                    self.len -= 1;
                    unsafe {
                        std::ptr::drop_in_place(item as *mut T);
                    }
                } else {
                    unsafe { 
                        std::ptr::copy_nonoverlapping(
                            item,
                            self.vec.as_mut_ptr().add(self.index),
                            1
                        );
                    }

                    self.forwards = true;
                    self.index += 1;

                    (&mut self.vec[self.index - 1]).update_pointer();
                }
            }
        }

        let guard = RetainGuard {
            f: |t| !(self.f)(t),
            vec: self.vec,
            should_swap: true,
            index: self.index,
            len: self.len,
        };
        guard.retain();
    }
}

impl<'a, T, F> Iterator for DrainFilter<'a, T, F>
where
    T: Unsticky,
    F: FnMut(&mut T) -> bool
{
    type Item = T::UnstuckType;

    fn next(&mut self) -> Option<Self::Item> {
        if self.forwards {
            while self.index < self.len {
                let item = &mut self.vec[self.index];

                if (self.f)(item) {
                    self.forwards = false;
                    self.len -= 1;
                    return Some(unsafe { std::ptr::read(item as *mut T) }.unstick())
                } else {
                    self.index += 1;
                }
            }
        } else {
            if self.index < self.len {
                let item = &mut self.vec[self.len];

                if (self.f)(item) {
                    self.len -= 1;
                    return Some(unsafe { std::ptr::read(item as *mut T) }.unstick())
                } else {
                    unsafe { 
                        std::ptr::copy_nonoverlapping(
                            item,
                            self.vec.as_mut_ptr().add(self.index),
                            1
                        );
                    }

                    self.forwards = true;
                    self.index += 1;

                    (&mut self.vec[self.index - 1]).update_pointer();

                    return self.next();
                }
            }
        }
        None
    }
}
