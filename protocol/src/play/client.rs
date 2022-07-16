use binary::slice_serialization::*;

use crate::identify_packets;
use crate::types::ArmPosition;
use crate::types::BlockPosition;
use crate::types::ChatVisibility;
use crate::types::Direction;
use crate::types::HandAction;
use crate::types::MoveAction;
use crate::IdentifiedPacket;
use num_enum::TryFromPrimitive;

identify_packets! {
    PacketId,
    AcceptTeleportation = 0x00,
    ChatCommand<'_> = 0x03,
    ClientInformation<'_> = 0x07,
    CustomPayload<'_> = 0x0c,
    KeepAlive = 0x11,
    MovePlayerPos = 0x13,
    MovePlayerPosRot = 0x14,
    MovePlayerRot = 0x15,
    PlayerHandAction = 0x1c,
    PlayerMoveAction = 0x1d
}

// Accept Teleportation

slice_serializable_composite! {
    AcceptTeleportation,
    id: i32 as VarInt
}

// Chat Command

slice_serializable_composite! {
    Signature<'a>,
    string: &'a str as SizedString<256>,
    bytes: &'a [u8] as SizedBlob<16> // 32?
}

slice_serializable_composite! {
    ChatCommand<'a>,
    command: &'a str as SizedString<256>,
    timestamp: u64 as BigEndian,
    salt: u64 as BigEndian,
    signatures: Vec<Signature<'a>> as SizedArray<Signature<'_>>,
    signed: bool as Single
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
    PlayerHandAction,
    action: HandAction as AttemptFrom<Single, u8>,
    block_pos: BlockPosition,
    direction: Direction as AttemptFrom<Single, u8>,
    sequence: i32 as VarInt
}

slice_serializable_composite! {
    PlayerMoveAction,
    id: i32 as VarInt,
    action: MoveAction as AttemptFrom<Single, u8>,
    data: i32 as VarInt,
}
