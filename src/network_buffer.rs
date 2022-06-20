use thiserror::Error;

use crate::varint;

pub enum PacketReadResult<'a> {
    Complete(&'a [u8]),
    Partial(&'a [u8]),
    Empty
}

pub struct PacketReadBuffer {
    pub vec: Vec<u8>,
    pub reader_index: usize,
    pub writer_index: usize
}

#[derive(Error, Debug)]
pub enum PacketReadBufferError {
    #[error("received packet exceeds maximum size of 2097148")]
    PacketTooBig
}

impl PacketReadBuffer {
    const INITIAL_SIZE: usize = 256;
    const GROWTH_FACTOR: usize = 2;
    const MAXIMUM_BUFFER_SIZE: usize = 2097152;
    const MAXIMUM_PACKET_SIZE: usize = 2097148;

    pub fn new() -> PacketReadBuffer {
        PacketReadBuffer {vec: vec![0u8; PacketReadBuffer::INITIAL_SIZE], reader_index: 0, writer_index: 0}
    }

    pub fn read_all<T: std::io::Read>(&mut self, reader: &mut T) -> anyhow::Result<()> {
        self.reader_index = 0;
        self.writer_index = reader.read(&mut self.vec)?;

        // todo: implement automatic truncation when capacity hasn't been utilised for some time

        // Allow growth up to ~2MB
        while self.writer_index == self.vec.len() && self.vec.len() * PacketReadBuffer::GROWTH_FACTOR <= PacketReadBuffer::MAXIMUM_BUFFER_SIZE {
            let read_from = self.vec.len();
            self.vec.resize(self.vec.len() * PacketReadBuffer::GROWTH_FACTOR, 0);

            self.writer_index += reader.read(&mut self.vec[read_from..])?;
        }

        Ok(())
    }

    pub fn try_read_packet(&mut self) -> anyhow::Result<PacketReadResult> {
        let remaining = self.writer_index - self.reader_index;

        if remaining == 0 {
            return Ok(PacketReadResult::Empty);
        } else if remaining >= 3 {
            // Packet must start with varint header specifying the amount of data
            let (packet_size, varint_header_bytes) = varint::decode::u21(&self.vec[self.reader_index..])?;
            let packet_size = packet_size as usize;

            if packet_size > PacketReadBuffer::MAXIMUM_PACKET_SIZE {
                return Err(PacketReadBufferError::PacketTooBig.into());
            }

            let remaining = self.writer_index - (self.reader_index + varint_header_bytes);
            if remaining >= packet_size {
                // Enough bytes to fully read, consume varint header & emit fully read packet
                self.reader_index += varint_header_bytes; // consume varint header
                let start = self.reader_index; // mark start of packet
                self.reader_index += packet_size; // advance to end of packet
                return Ok(PacketReadResult::Complete(&self.vec[start..self.reader_index]));
            }
        } else if remaining == 2 && self.vec[self.reader_index] == 1 { // Special case for packet of size 1
            // Enough bytes (2) to fully read
            self.reader_index += 1; // consume varint header (1 byte)
            let start = self.reader_index; // mark start of packet
            self.reader_index += 1; // advance to end of packet (+1 byte)
            return Ok(PacketReadResult::Complete(&self.vec[start..self.reader_index]));
        }

        // Not enough bytes to fully read, emit [varint header + remaining data] as partial read
        let start = self.reader_index; // mark start
        self.reader_index = self.writer_index; // advance to end
        Ok(PacketReadResult::Partial(&self.vec[start..self.reader_index]))
    }
}

// todo: remove when happy with varint.rs
// reads a varint, consuming a maximum of 3 bytes
/*unsafe fn read_varint3_unsafe(bytes: *const u8) -> (u32, usize) {
    let b = bytes.cast::<u32>().read_unaligned() & 0xffffff;
    let msbs = !b & !0x7f7f7f;
    let len = msbs.trailing_zeros() + 1; // in bits
    let varint_part = b & (msbs ^ msbs.wrapping_sub(1));

    let num = ((varint_part & 0x0000007f)
        | ((varint_part & 0x007f0000) >> 2)
        | ((varint_part & 0x00007f00) >> 1)) as u32;

    (num, (len / 8) as usize)
}*/