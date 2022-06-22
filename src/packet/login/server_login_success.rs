use crate::packet::Packet;
use crate::binary::{slice_reader, slice_writer};

#[derive(Debug)]
pub struct ServerLoginSuccess<'a> {
    pub uuid: u128,
    pub username: &'a str
}

impl <'a> Packet<'a, super::ServerPacketId> for ServerLoginSuccess<'a> {
    fn read(bytes: &'a [u8]) -> anyhow::Result<ServerLoginSuccess<'a>> {
        let mut bytes = bytes;

        let packet = ServerLoginSuccess {
            uuid: slice_reader::read_u128(&mut bytes)?,
            username: slice_reader::read_string_with_max_size(&mut bytes, 16)?,
        };

        // todo: verify integrity of username ([a-zA-Z0-9_]{3,16})

        slice_reader::ensure_fully_read(bytes)?;

        Ok(packet)
    }

    fn get_write_size(&self) -> usize {
        16 + 5 + self.username.len()
    }

    unsafe fn write<'b>(&self, mut bytes: &'b mut [u8]) -> &'b mut [u8] {
        bytes = slice_writer::write_u128(bytes, self.uuid);
        bytes = slice_writer::write_sized_string(bytes, self.username);

        bytes
    }
}