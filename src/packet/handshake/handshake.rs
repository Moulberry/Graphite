use crate::binary_reader;

pub struct Handshake<'a> {
    pub protocol_version: i32,
    pub server_address: &'a str,
    pub server_port: u16,
    pub next_state: i32
}

impl <'a> Handshake<'a> {
    pub fn read(bytes: &'a [u8]) -> anyhow::Result<Handshake<'a>> {
        let mut bytes = bytes;

        let packet = Handshake {
            protocol_version: binary_reader::read_varint(&mut bytes)?,
            server_address: binary_reader::read_string_with_max_size(&mut bytes, 256)?,
            server_port: binary_reader::read_u16(&mut bytes)?,
            next_state: binary_reader::read_varint(&mut bytes)?,
        };

        binary_reader::ensure_fully_read(bytes)?;

        Ok(packet)
    }
}