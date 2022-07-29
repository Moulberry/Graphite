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
    ChatCommand<'_> = 0x04,
    ClientInformation<'_> = 0x08,
    CustomPayload<'_> = 0x0d,
    KeepAlive = 0x12,
    MovePlayerPos = 0x14,
    MovePlayerPosRot = 0x15,
    MovePlayerRot = 0x16,
    PlayerHandAction = 0x1d,
    PlayerMoveAction = 0x1e
}

// Accept Teleportation

slice_serializable! {
    #[derive(Debug)]
    pub struct AcceptTeleportation {
        pub id: i32 as VarInt
    }
}

// Chat Command

slice_serializable! {
    #[derive(Debug)]
    pub struct Signature<'a> {
        pub string: &'a str as SizedString<256>,
        pub bytes: &'a [u8] as SizedBlob<16> // 300?
    }
}

slice_serializable! {
    #[derive(Debug)]
    pub struct ReceivedMessage<'a> {
        pub from_uuid: u128 as BigEndian,
        pub bytes: &'a [u8] as SizedBlob<300>
    }
}

slice_serializable! {
    #[derive(Debug)]
    pub struct ChatCommand<'a> {
        pub command: &'a str as SizedString<256>,
        pub timestamp: u64 as BigEndian,
        pub salt: u64 as BigEndian,
        pub signatures: Vec<Signature<'a>> as SizedArray<Signature<'_>>,
        pub signed: bool as Single,
        pub last_seen_messages: Vec<ReceivedMessage<'a>> as SizedArray<ReceivedMessage>,
    
        // only set if the player didn't see the last message (eg. message is from someone they blocked)
        // the client still informs the server, for ordering reasons
        pub last_received_message: Option<ReceivedMessage<'a>> 
    }
}

// Client Information

slice_serializable! {
    #[derive(Debug)]
    pub struct ClientInformation<'a> {
        pub language: &'a str as SizedString<16>,
        pub view_distance: u8 as Single,
        pub chat_visibility: ChatVisibility as AttemptFrom<Single, u8>,
        pub chat_colors: bool as Single,
        pub model_customization: i8 as Single,
        pub arm_position: ArmPosition as AttemptFrom<Single, u8>,
        pub text_filtering_enabled: bool as Single,
        pub show_on_server_list: bool as Single
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

// Move Player

slice_serializable! {
    #[derive(Debug)]
    pub struct MovePlayerPos {
        pub x: f64 as BigEndian,
        pub y: f64 as BigEndian,
        pub z: f64 as BigEndian,
        pub on_ground: bool as Single
    }
}

slice_serializable! {
    #[derive(Debug)]
    pub struct MovePlayerRot {
        pub yaw: f32 as BigEndian,
        pub pitch: f32 as BigEndian,
        pub on_ground: bool as Single
    }
}

slice_serializable! {
    #[derive(Debug)]
    pub struct MovePlayerPosRot {
        pub x: f64 as BigEndian,
        pub y: f64 as BigEndian,
        pub z: f64 as BigEndian,
        pub yaw: f32 as BigEndian,
        pub pitch: f32 as BigEndian,
        pub on_ground: bool as Single
    }
}

// Player Action

slice_serializable! {
    #[derive(Debug)]
    pub struct PlayerHandAction {
        pub action: HandAction as AttemptFrom<Single, u8>,
        pub block_pos: BlockPosition,
        pub direction: Direction as AttemptFrom<Single, u8>,
        pub sequence: i32 as VarInt
    }
}

slice_serializable! {
    #[derive(Debug)]
    pub struct PlayerMoveAction {
        pub id: i32 as VarInt,
        pub action: MoveAction as AttemptFrom<Single, u8>,
        pub data: i32 as VarInt,
    }
}
