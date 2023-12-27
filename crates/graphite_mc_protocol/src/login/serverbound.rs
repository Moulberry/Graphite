use std::borrow::Cow;

use graphite_binary::slice_serialization::*;

use crate::identify_packets;
use crate::IdentifiedPacket;
use num_enum::TryFromPrimitive;

identify_packets! {
    PacketId,
    Hello<'_> = 0x00,
    LoginAcknowledged = 0x03
}

slice_serializable! {
    #[derive(Debug)]
    pub struct Hello<'a> {
        pub username: Cow<'a, str> as SizedString<16>,
        pub uuid: u128 as BigEndian
    }
}

slice_serializable! {
    #[derive(Debug)]
    pub struct LoginAcknowledged {}
}
