use std::borrow::Cow;

use graphite_binary::slice_serialization::*;

use crate::identify_packets;
use crate::types::GameProfile;
use crate::IdentifiedPacket;
use num_enum::TryFromPrimitive;

identify_packets! {
    PacketId,
    // LoginDisconnect = 0x0,
    // Hello = 0x1,
    LoginSuccess<'_> = 0x2,
    // LoginCompression = 0x3,
    CustomQuery<'_> = 0x4
    // CookieRequest = 0x5
}

slice_serializable! {
    #[derive(Debug)]
    pub struct LoginSuccess<'a> {
        pub profile: GameProfile<'a>
    }
}

slice_serializable! {
    #[derive(Debug)]
    pub struct CustomQuery<'a> {
        pub transaction_id: i32 as VarInt,
        pub channel: Cow<'a, str> as SizedString,
        pub payload: Cow<'a, [u8]> as GreedyBlob
    }
}
