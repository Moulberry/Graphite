use crate::packet::Packet;
use crate::binary_reader;
use crate::binary_writer::BinaryWritable;

#[derive(Debug)]
pub struct ServerResponse<'a> {
    pub json: &'a str
}

impl <'a> Packet<'a, super::PacketId> for ServerResponse<'a> {
    fn read(bytes: &'a [u8]) -> anyhow::Result<ServerResponse<'a>> {
        let mut bytes = bytes;

        let packet = ServerResponse {
            json: binary_reader::read_string(&mut bytes)?
        };

        binary_reader::ensure_fully_read(bytes)?;

        Ok(packet)
    }

    fn get_write_len_hint(&self) -> usize {
        5 + self.json.len()
    }

    fn write(&self, vec: &mut Vec<u8>) {
        vec.put_sized_string(self.json);
    }
}