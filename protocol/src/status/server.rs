use binary::slice_serializable::*;

use crate::identify_packets;
use crate::IdentifiedPacket;
use derive_try_from_primitive::TryFromPrimitive;

identify_packets! {
    PacketId,
    Response<'_> = 0x00
}

slice_serializable_composite! {
    Response<'a>,
    json: &'a str as SizedString
}
