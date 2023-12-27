use std::borrow::Cow;

use graphite_binary::nbt::CachedNBT;
use graphite_binary::slice_serialization::*;

use crate::identify_packets;
use crate::types::GameProfile;
use crate::IdentifiedPacket;
use num_enum::TryFromPrimitive;

identify_packets! {
    PacketId,
    Disconnect<'_> = 0x01,
    FinishConfiguration = 0x02,
    RegistryData<'_> = 0x05
}

slice_serializable! {
    #[derive(Debug)]
    pub struct Disconnect<'a> {
        pub profile: GameProfile<'a>
    }
}

slice_serializable! {
    #[derive(Debug)]
    pub struct FinishConfiguration {
    }
}

slice_serializable! {
    #[derive(Debug)]
    pub struct RegistryData<'a> {
        pub nbt: Cow<'a, CachedNBT> as NBTBlob
    }
}