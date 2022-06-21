use crate::packet::Packet;
use crate::binary_reader;
use crate::binary_writer::BinaryWritable;
use bytes::BufMut;

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

    fn get_write_len_hint(&self) -> usize {
        5 + 5 + self.server_address.len() + 2 + 5
    }

    fn write(&self, vec: &mut Vec<u8>) {
        vec.put_varint_i32(self.protocol_version);
        vec.put_sized_string(self.server_address);
        vec.put_u16(self.server_port);
        vec.put_varint_i32(self.next_state);
    }
}