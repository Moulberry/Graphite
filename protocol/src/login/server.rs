use binary::slice_serializable::*;

use crate::identify_packets;
use crate::IdentifiedPacket;
use derive_try_from_primitive::TryFromPrimitive;

identify_packets! {
    PacketId,
    LoginSuccess<'_> = 0x02
}

slice_serializable_composite! {
    LoginSuccess<'a>,
    uuid: u128 as BigEndian,
    username: &'a str as SizedString<16>,
    property_count: i32 as VarInt
}
