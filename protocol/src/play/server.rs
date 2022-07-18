use binary::slice_serialization::*;

use crate::identify_packets;
use crate::types::CommandNode;
use crate::IdentifiedPacket;
use num_enum::TryFromPrimitive;

identify_packets! {
    PacketId,
    Commands = 0x0f,
    CustomPayload<'_> = 0x16,
    KeepAlive = 0x20,
    LevelChunkWithLight<'_> = 0x21,
    Login<'_> = 0x25,
    SetPlayerPosition = 0x39,
    SetChunkCacheCenter = 0x4b,
    SystemChat<'_> = 0x62
}

// Commands

slice_serializable_composite! {
    Commands,
    nodes: Vec<CommandNode> as SizedArray<CommandNode>,
    root_index: i32 as VarInt
}

// Custom Payload
slice_serializable_composite! {
    CustomPayload<'a>,
    channel: &'a str as SizedString,
    data: &'a [u8] as GreedyBlob
}

// Keep Alive
slice_serializable_composite! {
    KeepAlive,
    id: u64 as BigEndian
}

// LevelChunkWithLight
slice_serializable_composite! {
    ChunkBlockData<'a>,
    heightmaps: &'a [u8] as GreedyBlob, // todo: actually nbt, don't use blob, doesn't have correct read semantics
    data: &'a [u8] as SizedBlob,
    block_entity_count: i32 as VarInt,
    // todo: block entities
    trust_edges: bool as Single
}

slice_serializable_composite! {
    ChunkLightData<'a>,
    sky_light_mask: Vec<u64> as SizedArray<BigEndian>,
    block_light_mask: Vec<u64> as SizedArray<BigEndian>,
    empty_sky_light_mask: Vec<u64> as SizedArray<BigEndian>,
    empty_block_light_mask: Vec<u64> as SizedArray<BigEndian>,
    sky_light_entries: Vec<&'a [u8]> as SizedArray<SizedBlob>,
    block_light_entries: Vec<&'a [u8]> as SizedArray<SizedBlob>
}

slice_serializable_composite! {
    LevelChunkWithLight<'a>,
    chunk_x: i32 as BigEndian,
    chunk_z: i32 as BigEndian,
    chunk_block_data: ChunkBlockData<'a>,
    chunk_light_data: ChunkLightData<'a>
}

// Login
slice_serializable_composite! {
    Login<'a>,
    entity_id: i32 as BigEndian,
    is_hardcore: bool as Single,
    gamemode: u8 as Single,
    previous_gamemode: i8 as Single,
    dimension_names: Vec<&'a str> as SizedArray<SizedString>,
    registry_codec: &'a [u8] as GreedyBlob, // todo: actually nbt, don't use blob, doesn't have correct read semantics
    dimension_type: &'a str as SizedString,
    dimension_name: &'a str as SizedString,
    hashed_seed: u64 as BigEndian,
    max_players: i32 as VarInt,
    view_distance: i32 as VarInt,
    simulation_distance: i32 as VarInt,
    reduced_debug_info: bool as Single,
    enable_respawn_screen: bool as Single,
    is_debug: bool as Single,
    is_flat: bool as Single,
    has_death_location: bool as Single // must be false
}

// Player Position
slice_serializable_composite! {
    SetPlayerPosition,
    x: f64 as BigEndian,
    y: f64 as BigEndian,
    z: f64 as BigEndian,
    yaw: f32 as BigEndian,
    pitch: f32 as BigEndian,
    relative_arguments: u8 as Single,
    id: i32 as VarInt,
    dismount_vehicle: bool as Single
}

// Set Chunk Cache Center
slice_serializable_composite! {
    SetChunkCacheCenter,
    chunk_x: i32 as VarInt,
    chunk_z: i32 as VarInt
}

// System Chat
slice_serializable_composite! {
    SystemChat<'a>,
    message: &'a str as SizedString,
    overlay: bool as Single
}

