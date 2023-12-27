use graphite_binary::slice_serialization::*;

use crate::identify_packets;
use crate::IdentifiedPacket;
use num_enum::TryFromPrimitive;

identify_packets! {
    PacketId,
    StatusRequest = 0x00,
    PingRequest = 0x01
}

slice_serializable! {
    #[derive(Debug)]
    pub struct StatusRequest {}
}

slice_serializable! {
    #[derive(Debug)]
    pub struct PingRequest {
        pub time: u64 as BigEndian
    }
}
