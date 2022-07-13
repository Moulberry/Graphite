use binary::slice_serialization::*;

use crate::identify_packets;
use crate::IdentifiedPacket;
use num_enum::TryFromPrimitive;

identify_packets! {
    PacketId,
    Intention<'_> = 0x00
}

slice_serializable_composite! {
    Intention<'a>,
    protocol_version: i32 as VarInt,
    host_name: &'a str as SizedString<256>,
    port: u16 as BigEndian,
    intention: i32 as VarInt
}
