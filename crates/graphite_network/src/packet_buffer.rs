use graphite_binary::slice_serialization::SliceSerializable;
use graphite_mc_protocol::IdentifiedPacket;
use thiserror::Error;

#[derive(Clone, Debug)]
pub struct PacketBuffer {
    vec: Vec<u8>,
    write_index: usize,
}

impl Default for PacketBuffer {
    fn default() -> Self {
        Self {
            vec: Vec::new(),
            write_index: 0,
        }
    }
}

// todo: add tests for this type

#[derive(Error, Debug, Clone)]
pub enum PacketWriteError {
    #[error("Packet is more than 2097148 bytes")]
    PacketTooLarge
}

impl PacketBuffer {
    pub fn new() -> PacketBuffer {
        Default::default()
    }

    pub fn with_min_capacity(min_capacity: usize) -> PacketBuffer {
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

    pub fn clear(&mut self) {
        self.write_index = 0;
    }

    pub fn pop_written(&mut self) -> &[u8] {
        let ptr = self.vec.as_ptr();
        let slice = unsafe { std::slice::from_raw_parts(ptr, self.write_index) };
        self.clear();
        slice
    }

    pub fn write_packet<'a, I: std::fmt::Debug, T>(&mut self, packet: &'a T) -> Result<(), PacketWriteError>
    where
        T: SliceSerializable<'a, T> + IdentifiedPacket<I> + 'a,
    {
        self.write_serializable(packet.get_packet_id_as_u8(), packet)
    }

    pub fn write_serializable<'a, T>(&mut self, packet_id: u8, serializable: &'a T) -> Result<(), PacketWriteError>
    where
        T: SliceSerializable<'a, T>,
    {
        let expected_packet_size = T::get_write_size(T::as_copy_type(serializable));
        if expected_packet_size > 2097148 {
            return Err(PacketWriteError::PacketTooLarge);
        }

        if expected_packet_size <= 126 {
            let bytes = self.get_unwritten(2 + expected_packet_size);
            let slice_after_writing = unsafe { T::write(&mut bytes[2..], T::as_copy_type(serializable)) };

            let bytes_written = expected_packet_size - slice_after_writing.len();
            let packet_length_header = 1 + bytes_written;

            debug_assert!(packet_length_header <= 127);

            bytes[0] = packet_length_header as u8;
            bytes[1] = packet_id;

            unsafe {
                self.advance(2 + bytes_written);
            }
        } else {
            let bytes = self.get_unwritten(4 + expected_packet_size);
            let slice_after_writing = unsafe { T::write(&mut bytes[4..], T::as_copy_type(serializable)) };

            let bytes_written = expected_packet_size - slice_after_writing.len();
            let packet_length_header = 1 + bytes_written;

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
                return Err(PacketWriteError::PacketTooLarge);
            }

            // write packet id
            bytes[3] = packet_id;

            unsafe {
                self.advance(4 + bytes_written);
            }
        }

        Ok(())
    }

    fn get_unwritten(&mut self, capacity: usize) -> &mut [u8] {
        let current_requested_capacity = self.write_index + capacity;
        self.vec.reserve(current_requested_capacity);

        unsafe {
            let ptr = self.vec.as_mut_ptr().add(self.write_index);
            std::slice::from_raw_parts_mut(ptr, capacity)
        }
    }

    /// This function should be used after successfully writing some data with `get_unwritten`
    ///
    /// # Safety
    /// 1. `advance` must be less than the capacity requested in `get_unwritten`
    /// 2.  At least `advance` bytes must have been written to the slice returned by `get_unwritten`,
    ///     otherwise `get_written` will return uninitialized memory
    unsafe fn advance(&mut self, advance: usize) {
        debug_assert!(
            self.write_index + advance <= self.vec.capacity(),
            "advance {} must be <= the remaining bytes {}",
            advance,
            self.vec.capacity() - self.write_index
        );
        self.write_index += advance;
    }
}
