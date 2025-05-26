use graphite_binary::nbt::EncodedNBT;
use graphite_binary::slice_serialization::*;

use crate::identify_packets;
use crate::types::GameProfile;
use crate::IdentifiedPacket;
use num_enum::TryFromPrimitive;

identify_packets! {
    PacketId,
    // CookieRequest = 0x0,
    // CustomPayload = 0x1,
    Disconnect = 0x2,
    FinishConfiguration = 0x3,
    // KeepAlive = 0x4,
    // Ping = 0x5,
    // ResetChat = 0x6,
    RegistryData<'_> = 0x7
    // ResourcePackPop = 0x8,
    // ResourcePackPush = 0x9,
    // StoreCookie = 0xa,
    // Transfer = 0xb,
    // UpdateEnabledFeatures = 0xc,
    // UpdateTags = 0xd,
    // SelectKnownPacks = 0xe
}

slice_serializable! {
    #[derive(Debug)]
    pub struct Disconnect {
        pub message: EncodedNBT as NBTBlob
    }
}

slice_serializable! {
    #[derive(Debug)]
    pub struct FinishConfiguration {
    }
}


slice_serializable! {
    #[derive(Debug)]
    pub struct PackedRegistryEntry<'a> {
        pub id: &'a str as SizedString,
        pub data: Option<EncodedNBT> as Option<NBTBlob>
    }
}

slice_serializable! {
    #[derive(Debug)]
    pub struct RegistryData<'a> {
        pub registry: &'a str as SizedString,
        pub entries: Vec<PackedRegistryEntry<'a>> as SizedArray<PackedRegistryEntry<'a>>
    }
}