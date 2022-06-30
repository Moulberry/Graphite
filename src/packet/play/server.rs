use crate::binary::slice_serializable::*;

use crate::packet::identify_packets;
use crate::packet::IdentifiedPacket;
use derive_try_from_primitive::TryFromPrimitive;

identify_packets! {
    PacketId,
    PluginMessage<'_> = 0x15,
    ChunkDataAndUpdateLight<'_> = 0x1F,
    JoinGame<'_> = 0x23,
    PlayerPositionAndLook = 0x36,
    UpdateViewPosition = 0x48
}

slice_serializable_composite! {
    PluginMessage<'a>,
    channel: &'a str as SizedString,
    data: &'a [u8] as GreedyBlob
}

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
    sky_light_mask: Vec<u64> as SizedArray<BigEndian, _>,
    block_light_mask: Vec<u64> as SizedArray<BigEndian, _>,
    empty_sky_light_mask: Vec<u64> as SizedArray<BigEndian, _>,
    empty_block_light_mask: Vec<u64> as SizedArray<BigEndian, _>,
    sky_light_entries: Vec<&'a [u8]> as SizedArray<SizedBlob, _>,
    block_light_entries: Vec<&'a [u8]> as SizedArray<SizedBlob, _>
}

slice_serializable_composite! {
    ChunkDataAndUpdateLight<'a>,
    chunk_x: i32 as BigEndian,
    chunk_z: i32 as BigEndian,
    chunk_block_data: ChunkBlockData<'a>,
    chunk_light_data: ChunkLightData<'a>
}

slice_serializable_composite! {
    JoinGame<'a>,
    entity_id: i32 as BigEndian,
    is_hardcore: bool as Single,
    gamemode: u8 as Single,
    previous_gamemode: i8 as Single,
    dimension_names: Vec<&'a str> as SizedArray<SizedString, _>,
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

slice_serializable_composite! {
    PlayerPositionAndLook,
    x: f64 as BigEndian,
    y: f64 as BigEndian,
    z: f64 as BigEndian,
    yaw: f32 as BigEndian,
    pitch: f32 as BigEndian,
    flags: u8 as Single,
    teleport_id: i32 as VarInt,
    dismount_vehicle: bool as Single
}

slice_serializable_composite! {
    UpdateViewPosition,
    chunk_x: i32 as VarInt,
    chunk_z: i32 as VarInt
}