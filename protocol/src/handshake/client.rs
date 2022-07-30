use binary::slice_serialization::*;

use crate::identify_packets;
use crate::IdentifiedPacket;
use num_enum::TryFromPrimitive;

identify_packets! {
    PacketId,
    Intention<'_> = 0x00
}

slice_serializable! {
    #[derive(Debug)]
    pub struct Intention<'a> {
        pub protocol_version: i32 as VarInt,
        pub host_name: &'a str as SizedString<256>,
        pub port: u16 as BigEndian,
        pub intention: i32 as VarInt
    }
}
