use crate::binary::slice_serializable::*;

slice_serializable_composite!(
    ClientHandshake<'a>,
    protocol_version: i32 as VarInt,
    server_address: &'a str as SizedStringWithMax<256>,
    server_port: u16 as BigEndian,
    next_state: i32 as VarInt
);