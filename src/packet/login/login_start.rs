use crate::packet::Packet;
use crate::binary_reader;
use crate::binary_writer;

#[derive(Debug)]
pub struct LoginStart<'a> {
    pub username: &'a str
}

impl <'a> Packet<'a, super::PacketId> for LoginStart<'a> {
    fn read(bytes: &'a [u8]) -> anyhow::Result<LoginStart<'a>> {
        let mut bytes = bytes;

        let packet = LoginStart {
            username: binary_reader::read_string_with_max_size(&mut bytes, 16)?,
        };

        binary_reader::ensure_fully_read(bytes)?;

        Ok(packet)
    }

    fn get_write_size(&self) -> usize {
        5 + self.username.len()
    }

    unsafe fn write<'b>(&self, mut bytes: &'b mut [u8]) -> &'b mut [u8] {
        bytes = binary_writer::write_sized_string(bytes, self.username);

        bytes
    }
}