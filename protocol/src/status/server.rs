use binary::slice_serialization::*;

use crate::identify_packets;
use crate::IdentifiedPacket;
use num_enum::TryFromPrimitive;

identify_packets! {
    PacketId,
    StatusResponse<'_> = 0x00,
    PongResponse = 0x01
}

slice_serializable_composite! {
    StatusResponse<'a>,
    json: &'a str as SizedString
}

slice_serializable_composite! {
    PongResponse,
    time: u64 as BigEndian
}
