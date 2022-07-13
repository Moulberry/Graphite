use binary::slice_serialization::*;

use crate::identify_packets;
use crate::types::Action;
use crate::types::ArmPosition;
use crate::types::BlockPosition;
use crate::types::ChatVisibility;
use crate::types::Direction;
use crate::IdentifiedPacket;
use num_enum::TryFromPrimitive;

identify_packets! {
    PacketId,
    AcceptTeleportation = 0x00,
    ClientInformation<'_> = 0x07,
    CustomPayload<'_> = 0x0c,
    KeepAlive = 0x11,
    MovePlayerPos = 0x13,
    MovePlayerPosRot = 0x14,
    MovePlayerRot = 0x15,
    PlayerAction = 0x1c
}

// Accept Teleportation

slice_serializable_composite! {
    AcceptTeleportation,
    id: i32 as VarInt
}

// Client Information

slice_serializable_composite! {
    ClientInformation<'a>,
    language: &'a str as SizedString<16>,
    view_distance: u8 as Single,
    chat_visibility: ChatVisibility as AttemptFrom<Single, u8>,
    chat_colors: bool as Single,
    model_customization: i8 as Single,
    arm_position: ArmPosition as AttemptFrom<Single, u8>,
    text_filtering_enabled: bool as Single,
    show_on_server_list: bool as Single
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

// Move Player

slice_serializable_composite! {
    MovePlayerPos,
    x: f64 as BigEndian,
    y: f64 as BigEndian,
    z: f64 as BigEndian,
    on_ground: bool as Single
}

slice_serializable_composite! {
    MovePlayerRot,
    yaw: f32 as BigEndian,
    pitch: f32 as BigEndian,
    on_ground: bool as Single
}

slice_serializable_composite! {
    MovePlayerPosRot,
    x: f64 as BigEndian,
    y: f64 as BigEndian,
    z: f64 as BigEndian,
    yaw: f32 as BigEndian,
    pitch: f32 as BigEndian,
    on_ground: bool as Single
}

// Player Action

slice_serializable_composite! {
    PlayerAction,
    action: Action as AttemptFrom<Single, u8>,
    block_pos: BlockPosition,
    direction: Direction as AttemptFrom<Single, u8>,
    sequence: i32 as VarInt
}
