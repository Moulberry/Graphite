use graphite_binary::slice_serialization::*;

use crate::identify_packets;
use crate::IdentifiedPacket;
use num_enum::TryFromPrimitive;

identify_packets! {
    PacketId,
    // ClientInformation = 0x0,
    // CookieResponse = 0x1,
    // CustomPayload = 0x2,
    FinishConfiguration = 0x3
    // KeepAlive = 0x4,
    // Pong = 0x5,
    // ResourcePack = 0x6,
    // SelectKnownPacks = 0x7
}

slice_serializable! {
    #[derive(Debug)]
    pub struct FinishConfiguration {
    }
}
