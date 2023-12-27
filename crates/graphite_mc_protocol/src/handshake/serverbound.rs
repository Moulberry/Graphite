use std::borrow::Cow;

use graphite_binary::slice_serialization::*;
use num_enum::IntoPrimitive;

use crate::identify_packets;
use crate::IdentifiedPacket;
use num_enum::TryFromPrimitive;

identify_packets! {
    PacketId,
    Intention<'_> = 0x00
}

#[derive(Debug, Copy, Clone, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum IntentionType {
    Status = 0x1,
    Login = 0x2,
}

slice_serializable! {
    #[derive(Debug)]
    pub struct Intention<'a> {
        pub protocol_version: i32 as VarInt,
        pub host_name: Cow<'a, str> as SizedString<256>,
        pub port: u16 as BigEndian,
        pub intention: IntentionType as AttemptFrom<Single, u8>
    }
}