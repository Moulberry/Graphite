use binary::slice_serialization::*;

use crate::identify_packets;
use crate::types::{
    BlockPosition, ByteRotation, CommandNode, GameProfile, QuantizedShort, SignatureData,
};
use crate::IdentifiedPacket;
use num_enum::TryFromPrimitive;

identify_packets! {
    PacketId,
    AddEntity = 0x00,
    AddPlayer = 0x02,
    Commands = 0x0f,
    CustomPayload<'_> = 0x16,
    KeepAlive = 0x20,
    LevelChunkWithLight<'_> = 0x21,
    Login<'_> = 0x25,
    PlayerInfo<'_> = 0x37,
    RemoveEntities = 0x3b,
    SetPlayerPosition = 0x39,
    RotateHead = 0x3f,
    SetChunkCacheCenter = 0x4b,
    SystemChat<'_> = 0x62,
    TeleportEntity = 0x66
}

// Add Entity

slice_serializable! {
    #[derive(Debug)]
    pub struct AddEntity {
        pub id: i32 as VarInt,
        pub uuid: u128 as BigEndian,
        pub entity_type: i32 as VarInt,
        pub x: f64 as BigEndian,
        pub y: f64 as BigEndian,
        pub z: f64 as BigEndian,
        pub yaw: f32 as ByteRotation,
        pub pitch: f32 as ByteRotation,
        pub head_yaw: f32 as ByteRotation,
        pub data: i32 as VarInt, // nice naming mojang
        pub x_vel: f32 as QuantizedShort,
        pub y_vel: f32 as QuantizedShort,
        pub z_vel: f32 as QuantizedShort,
    }
}

// Add Player

slice_serializable! {
    #[derive(Debug)]
    pub struct AddPlayer {
        pub id: i32 as VarInt,
        pub uuid: u128 as BigEndian,
        pub x: f64 as BigEndian,
        pub y: f64 as BigEndian,
        pub z: f64 as BigEndian,
        pub yaw: f32 as ByteRotation,
        pub pitch: f32 as ByteRotation
    }
}

// Commands

slice_serializable! {
    #[derive(Debug)]
    pub struct Commands {
        pub nodes: Vec<CommandNode> as SizedArray<CommandNode>,
        pub root_index: i32 as VarInt
    }
}

// Custom Payload
slice_serializable! {
    #[derive(Debug)]
    pub struct CustomPayload<'a> {
        pub channel: &'a str as SizedString,
        pub data: &'a [u8] as GreedyBlob
    }
}

// Keep Alive
slice_serializable! {
    #[derive(Debug)]
    pub struct KeepAlive {
        pub id: u64 as BigEndian
    }
}

// LevelChunkWithLight
slice_serializable! {
    #[derive(Debug)]
    pub struct ChunkBlockData<'a> {
        pub heightmaps: &'a [u8] as NBTBlob,
        pub data: &'a [u8] as SizedBlob,
        pub block_entity_count: i32 as VarInt,
        // todo: block entities
        pub trust_edges: bool as Single
    }
}

slice_serializable! {
    #[derive(Debug)]
    pub struct ChunkLightData<'a> {
        pub sky_light_mask: Vec<u64> as SizedArray<BigEndian>,
        pub block_light_mask: Vec<u64> as SizedArray<BigEndian>,
        pub empty_sky_light_mask: Vec<u64> as SizedArray<BigEndian>,
        pub empty_block_light_mask: Vec<u64> as SizedArray<BigEndian>,
        pub sky_light_entries: Vec<&'a [u8]> as SizedArray<SizedBlob>,
        pub block_light_entries: Vec<&'a [u8]> as SizedArray<SizedBlob>
    }
}

slice_serializable! {
    #[derive(Debug)]
    pub struct LevelChunkWithLight<'a> {
        pub chunk_x: i32 as BigEndian,
        pub chunk_z: i32 as BigEndian,
        pub chunk_block_data: ChunkBlockData<'a>,
        pub chunk_light_data: ChunkLightData<'a>
    }
}

// Login
slice_serializable! {
    #[derive(Debug)]
    pub struct Login<'a> {
        pub entity_id: i32 as BigEndian,
        pub is_hardcore: bool as Single,
        pub gamemode: u8 as Single,
        pub previous_gamemode: i8 as Single,
        pub dimension_names: Vec<&'a str> as SizedArray<SizedString>,
        pub registry_codec: &'a [u8] as NBTBlob,
        pub dimension_type: &'a str as SizedString,
        pub dimension_name: &'a str as SizedString,
        pub hashed_seed: u64 as BigEndian,
        pub max_players: i32 as VarInt,
        pub view_distance: i32 as VarInt,
        pub simulation_distance: i32 as VarInt,
        pub reduced_debug_info: bool as Single,
        pub enable_respawn_screen: bool as Single,
        pub is_debug: bool as Single,
        pub is_flat: bool as Single,
        pub death_location: Option<BlockPosition>
    }
}

// PlayerInfo
slice_serializable! {
    #[derive(Debug)]
    pub struct PlayerInfoAddPlayer<'a> {
        pub profile: GameProfile,
        pub gamemode: u8 as Single,
        pub ping: i32 as VarInt,
        pub display_name: Option<&'a str> as Option<SizedString>,
        pub signature_data: Option<SignatureData<'a>>
    }
}

slice_serializable! {
    #[derive(Debug)]
    pub struct PlayerInfoUpdateGamemode {
        pub uuid: u128 as BigEndian,
        pub gamemode: u8 as Single
    }
}

slice_serializable! {
    #[derive(Debug)]
    pub struct PlayerInfoUpdateLatency {
        pub uuid: u128 as BigEndian,
        pub ping: i32 as VarInt
    }
}

slice_serializable! {
    #[derive(Debug)]
    pub struct PlayerInfoDisplayName<'a> {
        pub uuid: u128 as BigEndian,
        pub display_name: Option<&'a str> as Option<SizedString>,
    }
}

slice_serializable! {
    #[derive(Debug)]
    pub enum PlayerInfo<'a> {
        AddPlayer {
            values: Vec<PlayerInfoAddPlayer<'a>> as SizedArray<PlayerInfoAddPlayer>
        },
        UpdateGameMode {
            values: Vec<PlayerInfoUpdateGamemode> as SizedArray<PlayerInfoUpdateGamemode>
        },
        UpdateLatency {
            values: Vec<PlayerInfoUpdateLatency> as SizedArray<PlayerInfoUpdateLatency>
        },
        UpdateDisplayName {
            values: Vec<PlayerInfoDisplayName<'a>> as SizedArray<PlayerInfoDisplayName>
        },
        RemovePlayer {
            uuids: Vec<u128> as SizedArray<BigEndian>,
        }
    }
}

// Remove Entities
slice_serializable! {
    #[derive(Debug)]
    pub struct RemoveEntities {
        pub entities: Vec<i32> as SizedArray<VarInt>
    }
}

// Player Position
slice_serializable! {
    #[derive(Debug)]
    pub struct SetPlayerPosition {
        pub x: f64 as BigEndian,
        pub y: f64 as BigEndian,
        pub z: f64 as BigEndian,
        pub yaw: f32 as BigEndian,
        pub pitch: f32 as BigEndian,
        pub relative_arguments: u8 as Single,
        pub id: i32 as VarInt,
        pub dismount_vehicle: bool as Single
    }
}

// Rotate Head
slice_serializable! {
    #[derive(Debug)]
    pub struct RotateHead {
        pub entity_id: i32 as VarInt,
        pub head_yaw: f32 as ByteRotation
    }
}

// Set Chunk Cache Center
slice_serializable! {
    #[derive(Debug)]
    pub struct SetChunkCacheCenter {
        pub chunk_x: i32 as VarInt,
        pub chunk_z: i32 as VarInt
    }
}

// System Chat
slice_serializable! {
    #[derive(Debug)]
    pub struct SystemChat<'a> {
        pub message: &'a str as SizedString,
        pub overlay: bool as Single
    }
}

// Teleport Entity
slice_serializable! {
    #[derive(Debug)]
    pub struct TeleportEntity {
        pub entity_id: i32 as VarInt,
        pub x: f64 as BigEndian,
        pub y: f64 as BigEndian,
        pub z: f64 as BigEndian,
        pub yaw: f32 as ByteRotation,
        pub pitch: f32 as ByteRotation,
        pub on_ground: bool as Single
    }
}
