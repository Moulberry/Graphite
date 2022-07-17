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

    pub fn is_empty(&self) -> bool {
        self.len == 0
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

    pub fn get(&mut self, index: usize) -> &mut T {
        assert!(index < self.len);

        let bucket_index = Self::get_bucket_index(index);
        let bucket = &mut self.buckets[bucket_index];

        let index_in_bucket = index + 8 - bucket.capacity();
        &mut bucket[index_in_bucket]
    }

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

    pub fn retain<F>(&mut self, mut f: F)
    where
        F: FnMut(&T) -> bool,
    {
        self.retain_mut(|elem| f(elem));
    }

    pub fn retain_mut<F>(&mut self, mut f: F)
    where
        F: FnMut(&mut T) -> bool,
    {
        let mut total_index: usize = 0;

        let mut bucket_index: usize = 0;
        while bucket_index < self.buckets.len() {
            let bucket: *mut Vec<T> = &mut self.buckets[bucket_index];
            let mut bucket_count = self.buckets.len();
            let mut bucket_length = unsafe { bucket.as_ref().unwrap() }.len();

            let mut item_index: usize = 0;
            while item_index < bucket_length {
                let item_ptr: *mut T =
                    unsafe { bucket.as_mut().unwrap().as_mut_ptr().add(item_index) };

                if !f(unsafe { item_ptr.as_mut().unwrap() }) {
                    self.len -= 1;

                    // Find suitable element for swap
                    'find_suitable: loop {
                        let bucket_index_last = bucket_count - 1;
                        if bucket_index_last == bucket_index {
                            // Check if we are already in the last bucket
                            loop {
                                bucket_length -= 1;
                                if item_index == bucket_length {
                                    // This was the last item!
                                    if bucket_length == 0 {
                                        // Bucket would be empty, lets just remove the entire bucket
                                        bucket_count -= 1;
                                        self.buckets.truncate(bucket_count);
                                    } else {
                                        // Truncate the bucket to the desired (non-zero) length
                                        unsafe { bucket.as_mut().unwrap() }.truncate(bucket_length);
                                    }
                                    return;
                                } else {
                                    debug_assert!(bucket_length > item_index);

                                    let item_last =
                                        &mut unsafe { bucket.as_mut().unwrap() }[bucket_length];

                                    // Check last item
                                    if f(item_last) {
                                        // Swap into item_index
                                        unsafe {
                                            std::ptr::drop_in_place(item_ptr);
                                            std::ptr::copy_nonoverlapping(item_last, item_ptr, 1);
                                            bucket.as_mut().unwrap().truncate(bucket_length + 1);
                                            bucket.as_mut().unwrap().set_len(bucket_length);
                                            item_ptr.as_mut().unwrap().update_pointer(total_index);
                                        }
                                        break 'find_suitable;
                                    } else {
                                        // Drop item and continue
                                        self.len -= 1;
                                    }
                                }
                            }
                        } else {
                            let bucket_last = &mut self.buckets[bucket_index_last];
                            let mut bucket_last_length = bucket_last.len();

                            while bucket_last_length > 0 {
                                let item_last = &mut bucket_last[bucket_last_length - 1];

                                if f(item_last) {
                                    // Swap into item_index
                                    unsafe {
                                        std::ptr::drop_in_place(item_ptr);
                                        std::ptr::copy_nonoverlapping(item_last, item_ptr, 1);
                                        bucket_last.truncate(bucket_last_length);
                                        bucket_last.set_len(bucket_last_length - 1);
                                        item_ptr.as_mut().unwrap().update_pointer(total_index);
                                    }
                                    break 'find_suitable;
                                } else {
                                    bucket_last_length -= 1;
                                    self.len -= 1;
                                }
                            }

                            // Last bucket is now empty, lets just remove the entire bucket
                            bucket_count -= 1;
                            self.buckets.truncate(bucket_count);
                        }
                    }
                }
                total_index += 1;
                item_index += 1;
            }
            bucket_index += 1;
        }
    }

    pub fn for_each<F>(&self, mut f: F)
    where
        F: FnMut(&T),
    {
        self.enumerate(|_, e| f(e));
    }

    pub fn for_each_mut<F>(&mut self, mut f: F)
    where
        F: FnMut(&mut T),
    {
        self.enumerate_mut(|_, e| f(e));
    }

    pub fn enumerate<F>(&self, mut f: F)
    where
        F: FnMut(usize, &T),
    {
        let mut overall_index = 0;
        for bucket_index in 0..self.buckets.len() {
            // Iterate through buckets
            let bucket = &self.buckets[bucket_index];

            // Iterate through items in bucket
            for item in bucket {
                f(overall_index, item); // Call function
                overall_index += 1;
            }
        }
    }

    pub fn enumerate_mut<F>(&mut self, mut f: F)
    where
        F: FnMut(usize, &mut T),
    {
        let mut overall_index = 0;
        for bucket_index in 0..self.buckets.len() {
            // Iterate through buckets
            let bucket = &mut self.buckets[bucket_index];

            // Iterate through items in bucket
            for item in bucket {
                f(overall_index, item); // Call function
                overall_index += 1;
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
