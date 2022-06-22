use crate::packet::Packet;
use crate::binary::{slice_reader, slice_writer};

#[derive(Debug)]
pub struct ServerJoinGame {
    pub entity_id: i32
}

impl <'a> Packet<'a, super::ServerPacketId> for ServerJoinGame {
    fn read(bytes: &'a [u8]) -> anyhow::Result<ServerJoinGame> {
        let mut bytes = bytes;

        let packet = ServerJoinGame {
            entity_id: slice_reader::read_i32(&mut bytes)?,
        };

        // todo: verify integrity of username ([a-zA-Z0-9_]{3,16})

        slice_reader::ensure_fully_read(bytes)?;

        Ok(packet)
    }

    fn get_write_size(&self) -> usize {
        4
    }

    unsafe fn write<'b>(&self, mut bytes: &'b mut [u8]) -> &'b mut [u8] {
        bytes = slice_writer::write_i32(bytes, self.entity_id);

        bytes
    }
}