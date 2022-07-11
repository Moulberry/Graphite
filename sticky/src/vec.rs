use crate::unsticky::Unsticky;

use std::fmt::Debug;

#[derive(Debug)]
pub struct StickyVec<T> {
    buckets: Vec<Vec<T>>,
    len: usize,
    next: usize,
}

impl<T> Default for StickyVec<T> {
    fn default() -> Self {
        Self {
            buckets: Default::default(),
            len: 0,
            next: 0,
        }
    }
}

impl<T: Unsticky> StickyVec<T> {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn insert(&mut self, t: T) {
        debug_assert!(self.buckets.len() >= self.next);

        // Allocate the bucket if needed
        if self.buckets.len() == self.next {
            self.buckets
                .push(Vec::with_capacity(Self::get_capacity_for_index(self.next)));
        }

        // Get the next bucket
        let mut bucket = &mut self.buckets[self.next];
        let len = bucket.len();

        debug_assert!(len < bucket.capacity(), "push should never reallocate");

        // Push the element into `current_vec`
        self.len += 1;
        bucket.push(t);

        // Return sticky pointer to pushed element
        let actual_index = bucket.capacity() - 8 + len;
        let ptr: &mut T = &mut bucket[len];
        ptr.update_pointer(actual_index);

        // Move to next chunk if we just filled `current_vec`
        if len == bucket.capacity() - 1 {
            self.next += 1;

            while self.buckets.len() < self.next {
                bucket = &mut self.buckets[self.next];
                if bucket.len() < bucket.capacity() {
                    break;
                }
            }
        }
    }

    pub(crate) fn get_bucket_index(index: usize) -> usize {
        std::mem::size_of::<usize>() * 8 - 4 - (index + 8).leading_zeros() as usize
    }

    fn get_capacity_for_index(index: usize) -> usize {
        1 << (index + 3)
    }
}

impl<T: Unsticky> StickyVec<T> {
    pub fn remove(&mut self, index: usize) -> T::UnstuckType {
        assert!(index < self.len);

        let last_bucket_index = self.buckets.len() - 1;
        let last_element_in_last_bucket = self.buckets[last_bucket_index].pop().unwrap();

        // Remove the last bucket if it is empty
        if self.buckets[last_bucket_index].is_empty() {
            self.buckets.remove(last_bucket_index);
        }

        if index == self.len - 1 {
            self.next = last_bucket_index;
            self.len -= 1;
            return last_element_in_last_bucket.unstick();
        }

        let bucket_index = Self::get_bucket_index(index);
        let bucket = &mut self.buckets[bucket_index];

        unsafe {
            let index_in_bucket = index + 8 - bucket.capacity();
            let dst = bucket.as_mut_ptr().add(index_in_bucket);

            // Read the value
            let value = std::ptr::read(dst);

            // Move `last_element_in_last_bucket` into the space previously occupied by `value`
            // panic!("index in bucket: {}, bucket id: {}", index_in_bucket, bucket_index);
            std::ptr::copy_nonoverlapping(&last_element_in_last_bucket, dst, 1);
            std::mem::forget(last_element_in_last_bucket); // Don't call drop

            // Update the pointer on the element we swapped in
            dst.as_mut().unwrap().update_pointer(index);

            self.next = bucket_index;
            self.len -= 1;

            value.unstick()
        }
    }
}

impl<T> Drop for StickyVec<T> {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(
                self.len == 0,
                "StickyVec must be drained before it can be dropped"
            );
        }
    }
}
