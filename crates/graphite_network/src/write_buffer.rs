#[derive(Clone, Debug)]
pub struct WriteBuffer {
    vec: Vec<u8>,
    write_index: usize,
}

impl Default for WriteBuffer {
    fn default() -> Self {
        Self {
            vec: Vec::new(),
            write_index: 0,
        }
    }
}

// todo: add tests for this type

impl WriteBuffer {
    pub fn new() -> WriteBuffer {
        Default::default()
    }

    pub fn with_min_capacity(min_capacity: usize) -> WriteBuffer {
        Self {
            vec: Vec::with_capacity(min_capacity),
            write_index: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.write_index
    }

    pub fn is_empty(&self) -> bool {
        self.write_index == 0
    }

    pub fn into_written(mut self) -> Vec<u8> {
        unsafe {
            self.vec.set_len(self.write_index);
        }
        self.vec
    }

    pub fn clear(&mut self) {
        self.write_index = 0;
    }

    pub fn pop_written(&mut self) -> Vec<u8> {
        let mut popped = std::mem::replace(&mut self.vec, Vec::new());
        unsafe { popped.set_len(self.write_index); }
        self.write_index = 0;
        popped
    }

    pub fn get_written(&self) -> &[u8] {
        let ptr = self.vec.as_ptr();
        unsafe { std::slice::from_raw_parts(ptr, self.write_index) }
    }

    pub fn get_unwritten(&mut self, capacity: usize) -> &mut [u8] {
        let current_requested_capacity = self.write_index + capacity;
        self.vec.reserve(current_requested_capacity);

        unsafe {
            let ptr = self.vec.as_mut_ptr().add(self.write_index);
            std::slice::from_raw_parts_mut(ptr, capacity)
        }
    }

    pub fn copy_from(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
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
