pub struct WriteBuffer {
    vec: Vec<u8>,
    write_index: usize
}

impl Default for WriteBuffer {
    fn default() -> Self {
        Self {
            vec: Vec::new(),
            write_index: 0
        }
    }
}

impl WriteBuffer {
    pub fn new() -> WriteBuffer {
        Default::default()
    }

    pub fn get_written(&self) -> &[u8] {
        let ptr = self.vec.as_ptr();
        unsafe { std::slice::from_raw_parts(ptr, self.write_index) }
    }

    pub fn get_unwritten(&mut self, capacity: usize) -> &mut [u8] {
        let needed = capacity as isize - self.vec.capacity() as isize + self.write_index as isize;
        if needed > 0 {
            self.vec.reserve(needed as usize);
        }

        let ptr = self.vec.as_mut_ptr();
        unsafe { std::slice::from_raw_parts_mut(ptr, capacity) }
    }

    /// This function should be used after successfully writing some data with `get_unwritten`
    /// 
    /// SAFETY:
    /// 1. `advance` must be less than the capacity requested in `get_unwritten`
    /// 2.  At least `advance` bytes must have been written to the slice returned by `get_unwritten`,
    ///     otherwise `get_written` will return uninitialized memory
    pub unsafe fn advance(&mut self, advance: usize) {
        debug_assert!(self.write_index + advance <= self.vec.capacity(), "advance must be less than the remaining bytes");
        self.write_index += advance;
    }
}