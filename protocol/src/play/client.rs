use binary::slice_serialization::*;

use crate::identify_packets;
use crate::IdentifiedPacket;
use derive_try_from_primitive::TryFromPrimitive;

identify_packets! {
    PacketId,
    SetPlayerPosition = 0x13
}

slice_serializable_composite! {
    SetPlayerPosition,
    x: f64 as BigEndian,
    y: f64 as BigEndian,
    z: f64 as BigEndian,
    on_ground: bool as Single
}