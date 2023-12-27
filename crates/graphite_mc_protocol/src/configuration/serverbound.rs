use graphite_binary::slice_serialization::*;

use crate::identify_packets;
use crate::types::GameProfile;
use crate::IdentifiedPacket;
use num_enum::TryFromPrimitive;

identify_packets! {
    PacketId,
    FinishConfiguration = 0x02
}

slice_serializable! {
    #[derive(Debug)]
    pub struct FinishConfiguration {
    }
}
