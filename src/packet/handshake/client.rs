use crate::binary::slice_serializable::*;

use crate::packet::identify_packets;
use crate::packet::IdentifiedPacket;
use derive_try_from_primitive::TryFromPrimitive;

identify_packets! {
    PacketId,
    Handshake<'_> = 0x00
}

slice_serializable_composite! {
    Handshake<'a>,
    protocol_version: i32 as VarInt,
    server_address: &'a str as SizedString<256>,
    server_port: u16 as BigEndian,
    next_state: i32 as VarInt
}
