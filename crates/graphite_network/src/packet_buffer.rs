use std::io::Write;

use graphite_binary::slice_serialization::SliceSerializable;

#[derive(Clone, Debug)]
pub struct PacketBuffer {
    vec: Vec<u8>,
    read_index: usize,
    write_index: usize,
}

impl PacketBuffer {
    pub fn new() -> PacketBuffer {
        Self {
            vec: Vec::new(),
            read_index: 0,
            write_index: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.write_index - self.read_index
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn clear(&mut self) {
        self.read_index = 0;
        self.write_index = 0;
    }

    pub fn pop_written(&mut self) -> &[u8] {
        if self.vec.capacity() == 0 {
            return &[];
        }

        let ptr = unsafe { self.vec.as_ptr().add(self.read_index) };
        let slice = unsafe { std::slice::from_raw_parts(ptr, self.len()) };
        self.clear();
        slice
    }

    pub fn peek_written(&self) -> &[u8] {
        if self.vec.capacity() == 0 {
            return &[];
        }

        let ptr = unsafe { self.vec.as_ptr().add(self.read_index) };
        unsafe { std::slice::from_raw_parts(ptr, self.len()) }
    }

    pub fn pop_written_into(&mut self, write: &mut impl Write) -> bool {
        if self.is_empty() {
            return true;
        }

        loop {
            let bytes = self.peek_written();
            match write.write(bytes) {
                // No longer able to accept bytes
                Ok(0) => {
                    return false;
                }
                // Partial write, try to keep going
                Ok(n) if n < bytes.len() => {
                    self.read_index += n;
                    continue;
                }
                // Success
                Ok(_) => {
                    self.clear();
                    return true;
                }
                // Not ready to write
                Err(ref err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    return true;
                }
                // Interrupted... try again
                Err(ref err) if err.kind() == std::io::ErrorKind::Interrupted => {
                    continue;
                }
                // Other errors we'll consider fatal.
                Err(err) => {
                    eprintln!("Error while sending bytes to connection: {}", err);
                    return false;
                },
            }
        }
    }

    pub fn copy_bytes(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }

        let unwritten = self.get_unwritten(bytes.len());
        unwritten.copy_from_slice(bytes);
        self.advance(bytes.len());
    }

    pub fn copy_from(&mut self, other: &PacketBuffer) {
        self.copy_bytes(other.peek_written());        
    }

    pub fn write_raw<'r, 'd: 'r, T>(&mut self, serializable: &'r T)
    where
        T: SliceSerializable<'r, 'd, T>,
    {
        let ref_type = T::as_copy_type(serializable);

        let expected_size = T::get_write_size(ref_type.clone());

        // allocate necessary bytes
        let bytes = self.get_unwritten(expected_size);

        // write the serializable
        let slice_after_writing = unsafe { T::write(bytes, ref_type) };
        let bytes_written = expected_size - slice_after_writing.len();

        // advance the write buffer
        self.advance(bytes_written);
    }

    pub fn write_serializable<'r, 'd: 'r, T>(&mut self, packet_id: u8, serializable: &'r T)
    where
        T: SliceSerializable<'r, 'd, T>,
    {
        let expected_packet_size = T::get_write_size(T::as_copy_type(serializable));
        
        self.write_custom(packet_id, expected_packet_size, |bytes| {
            unsafe { T::write(bytes, T::as_copy_type(serializable)) }
        })
    }

    pub fn write_custom(&mut self, packet_id: u8, expected_packet_size: usize, mut function: impl FnMut(&mut [u8]) -> &mut [u8]) {
        if expected_packet_size > 2097148 {
            return;
        }

        if expected_packet_size <= 126 {
            let bytes = self.get_unwritten(2 + expected_packet_size);
            let slice_after_writing = function(&mut bytes[2..]);

            let bytes_written = expected_packet_size - slice_after_writing.len();
            let packet_length_header = 1 + bytes_written;

            debug_assert!(bytes_written <= expected_packet_size);
            debug_assert!(packet_length_header <= 127);

            bytes[0] = packet_length_header as u8;
            bytes[1] = packet_id;

            self.advance(2 + bytes_written);
        } else {
            let bytes = self.get_unwritten(4 + expected_packet_size);
            let slice_after_writing = function(&mut bytes[4..]);

            let bytes_written = expected_packet_size - slice_after_writing.len();
            let packet_length_header = 1 + bytes_written;

            debug_assert!(bytes_written <= expected_packet_size);

            // write packet size varint, padded to 3 bytes
            if packet_length_header <= 127 {
                bytes[0] = 0b10000000 | packet_length_header as u8;
                bytes[1] = 0b10000000;
                bytes[2] = 0b00000000;
            } else if packet_length_header <= 16383 {
                bytes[0] = 0b10000000 | packet_length_header as u8;
                bytes[1] = 0b10000000 | (packet_length_header >> 7) as u8;
                bytes[2] = 0b00000000;
            } else if packet_length_header <= 2097151 {
                bytes[0] = 0b10000000 | packet_length_header as u8;
                bytes[1] = 0b10000000 | (packet_length_header >> 7) as u8;
                bytes[2] = (packet_length_header >> 14) as u8;
            } else {
                return;
            }

            // write packet id
            bytes[3] = packet_id;

            self.advance(4 + bytes_written);
        }
    }

    fn get_unwritten(&mut self, need: usize) -> &mut [u8] {
        if self.read_index == self.write_index {
            self.read_index = 0;
            self.write_index = 0;
        } else if self.read_index > 1_048_576 {
            let src = unsafe { self.vec.as_mut_ptr().add(self.read_index) };
            let dst = self.vec.as_mut_ptr();
            unsafe {
                std::ptr::copy(src, dst, self.len());
            }

            self.write_index -= self.read_index;
            self.read_index = 0;
        }

        let current_requested_capacity = (self.write_index + need).max(need * 2);
        self.vec.reserve(current_requested_capacity);

        unsafe {
            let ptr = self.vec.as_mut_ptr().add(self.write_index);
            std::slice::from_raw_parts_mut(ptr, need)
        }
    }

    /// This function should be used after successfully writing some data with `get_unwritten`
    fn advance(&mut self, advance: usize) {
        assert!(
            self.write_index + advance <= self.vec.capacity(),
            "advance {} must be <= the remaining bytes {}",
            advance,
            self.vec.capacity() - self.write_index
        );
        self.write_index += advance;
    }
}
