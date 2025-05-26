use std::borrow::Cow;

use graphite_binary::slice_serialization::*;

use crate::identify_packets;
use crate::IdentifiedPacket;
use num_enum::TryFromPrimitive;

identify_packets! {
    PacketId,
    Hello<'_> = 0x00,
    // Key = 0x01,
    CustomQueryAnswer<'_> = 0x02,
    LoginAcknowledged = 0x03
    // CookieResponse = 0x04
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
    pub struct CustomQueryAnswer<'a> {
        pub transaction_id: i32 as VarInt,
        pub payload: Option<Cow<'a, [u8]>> as Option<GreedyBlob>
    }
}

slice_serializable! {
    #[derive(Debug)]
    pub struct LoginAcknowledged {}
}
