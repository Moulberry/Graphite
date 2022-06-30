use crate::binary::slice_serializable::*;

use crate::packet::identify_packets;
use crate::packet::IdentifiedPacket;
use derive_try_from_primitive::TryFromPrimitive;

identify_packets! {
    PacketId,
    Response<'_> = 0x00
}

slice_serializable_composite! {
    Response<'a>,
    json: &'a str as SizedString
}
