const MIN_SIZE: usize = 1024;

#[derive(Clone, Debug)]
pub struct WriteBuffer {
    min_size: usize,
    vec: Vec<u8>,
    write_index: usize,
    shrink_counter: usize,
    current_requested_capacity: usize,
    max_requested_capacity: usize,
}

impl Default for WriteBuffer {
    fn default() -> Self {
        Self {
            min_size: MIN_SIZE,
            vec: Vec::with_capacity(MIN_SIZE),
            write_index: 0,
            current_requested_capacity: 0,
            max_requested_capacity: 0,
            shrink_counter: 0,
        }
    }
}

// todo: add tests for this type

impl WriteBuffer {
    pub fn with_min_capacity(min_capacity: usize) -> WriteBuffer {
        Self {
            min_size: min_capacity,
            vec: Vec::with_capacity(min_capacity),
            write_index: 0,
            current_requested_capacity: 0,
            max_requested_capacity: 0,
            shrink_counter: 0,
        }
    }

    pub fn into_written(mut self) -> Vec<u8> {
        unsafe { self.vec.set_len(self.write_index); }
        self.vec
    }

    // todo: remove this function and make every invocation specify the min capacity
    pub fn new() -> WriteBuffer {
        Default::default()
    }

    pub fn tick_and_maybe_shrink(&mut self) {
        self.shrink_counter += 1;

        if self.shrink_counter > 100 {
            self.shrink_counter = 0;

            self.vec.shrink_to(self.max_requested_capacity);
            self.max_requested_capacity = MIN_SIZE;
        }
    }

    pub fn reset(&mut self) {
        if self.current_requested_capacity > self.max_requested_capacity {
            self.max_requested_capacity = self.current_requested_capacity;
        }

        self.current_requested_capacity = 0;
        self.write_index = 0;
    }

    pub fn get_written(&self) -> &[u8] {
        let ptr = self.vec.as_ptr();
        unsafe { std::slice::from_raw_parts(ptr, self.write_index) }
    }

    pub fn get_unwritten(&mut self, capacity: usize) -> &mut [u8] {
        self.current_requested_capacity = self.write_index + capacity; // mark the current utilization

        self.vec.reserve(self.current_requested_capacity);

        unsafe {
            let ptr = self.vec.as_mut_ptr().add(self.write_index);
            std::slice::from_raw_parts_mut(ptr, capacity)
        }
    }

    pub fn copy_from(&mut self, bytes: &[u8]) {
        if bytes.len() == 0 {
            return;
        }

        self.get_unwritten(bytes.len()).copy_from_slice(bytes);
        unsafe {
            self.advance(bytes.len());
        }
    }

    /// This function should be used after successfully writing some data with `get_unwritten`
    ///
    /// # Safety
    /// 1. `advance` must be less than the capacity requested in `get_unwritten`
    /// 2.  At least `advance` bytes must have been written to the slice returned by `get_unwritten`,
    ///     otherwise `get_written` will return uninitialized memory
    pub unsafe fn advance(&mut self, advance: usize) {
        debug_assert!(
            self.write_index + advance <= self.vec.capacity(),
            "advance {} must be <= the remaining bytes {}",
            advance,
            self.vec.capacity() - self.write_index
        );
        self.write_index += advance;
    }
}
