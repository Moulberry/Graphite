use crate::packet::Packet;
use crate::binary_reader;
use crate::binary_writer;

#[derive(Debug)]
pub struct ClientHandshake<'a> {
    pub protocol_version: i32,
    pub server_address: &'a str,
    pub server_port: u16,
    pub next_state: i32
}

impl <'a> Packet<'a, super::PacketId> for ClientHandshake<'a> {
    fn read(bytes: &'a [u8]) -> anyhow::Result<ClientHandshake<'a>> {
        let mut bytes = bytes;

        let packet = ClientHandshake {
            protocol_version: binary_reader::read_varint(&mut bytes)?,
            server_address: binary_reader::read_string_with_max_size(&mut bytes, 256)?,
            server_port: binary_reader::read_u16(&mut bytes)?,
            next_state: binary_reader::read_varint(&mut bytes)?,
        };

        binary_reader::ensure_fully_read(bytes)?;

        Ok(packet)
    }

    fn get_write_size(&self) -> usize {
        5 + 5 + self.server_address.len() + 2 + 5
    }

    unsafe fn write<'b>(&self, mut bytes: &'b mut [u8]) -> &'b mut [u8] {
        bytes = binary_writer::write_varint_i32(bytes, self.protocol_version);
        bytes = binary_writer::write_sized_string(bytes, self.server_address);
        bytes = binary_writer::write_u16(bytes, self.server_port);
        bytes = binary_writer::write_varint_i32(bytes, self.next_state);

        bytes
    }
}