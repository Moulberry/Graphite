use graphite_binary::slice_serialization::*;

use crate::identify_packets;
use crate::types::ArmPosition;
use crate::types::BlockHitResult;
use crate::types::BlockPosition;
use crate::types::ChatVisibility;
use crate::types::Direction;
use crate::types::Hand;
use crate::types::HandAction;
use crate::types::MoveAction;
use crate::types::ProtocolItemStack;
use crate::IdentifiedPacket;
use num_enum::TryFromPrimitive;

identify_packets! {
    PacketId,
    AcceptTeleportation = 0x00,
    // BlockEntityTagQuery = 0x01,
    // ChangeDifficulty = 0x02,
    // ChatAck = 0x03,
    ChatCommand<'_> = 0x04,
    // Chat = 0x05,
    // PlayerSession = 0x06
    // ChunkBatchReceived = 0x07,
    // ClientStatus = 0x08,
    ClientInformation<'_> = 0x09,
    // CommandSuggestion = 0x0a,
    // AcknowledgeConfiguration = 0x0b,
    // ContainerButtonClick = 0x0c,
    // ContainerClick = 0x0d,
    // ContainerClose = 0x0e,
    // ChangeContainerSlotState = 0x0f,
    CustomPayload<'_> = 0x10,
    // EditBook = 0x11,
    // EntityTagQuery = 0x12,
    InteractEntity = 0x13,
    // JigsawGenerate = 0x14,
    KeepAlive = 0x15,
    // LockDifficulty = 0x16,
    MovePlayerPos = 0x17,
    MovePlayerPosRot = 0x18,
    MovePlayerRot = 0x19,
    MovePlayerOnGround = 0x1a,
    // MoveVehicle = 0x1b,
    // PaddleBoat = 0x1c,
    // PickItem = 0x1d,
    // PingRequest = 0x1e,
    // PlaceRecipe = 0x1f,
    PlayerAbilities = 0x20,
    PlayerHandAction = 0x21,
    PlayerMoveAction = 0x22,
    // PlayerInput = 0x23,
    // Pong = 0x24,
    // RecipeBookChangeSettings = 0x25,
    // RecipeBookSeenRecipe = 0x26,
    // RenameItem = 0x27,
    // ResourcePack = 0x28,
    // SeenAdvancements = 0x29,
    // SelectTrade = 0x2a,
    // SetBeaconEffect = 0x2b,
    SetCarriedItem = 0x2c,
    // SetCommandBlock = 0x2d,
    // SetCommandBlockMinecart = 0x2e,
    SetCreativeModeSlot<'_> = 0x2f,
    // SetJigsawBlock = 0x30,
    // SetStructureBlock = 0x31,
    // UpdateSign = 0x32,
    Swing = 0x33,
    // TeleportToEntity = 0x34,
    UseItemOn = 0x35,
    UseItem = 0x36
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

// Interact Entity

slice_serializable! {
    #[derive(Debug)]
    pub enum InteractMode {
        Interact {
            hand: Hand as AttemptFrom<Single, u8>
        },
        Attack {
        },
        InteractAt {
            offset_x: f32 as BigEndian,
            offset_y: f32 as BigEndian,
            offset_z: f32 as BigEndian,
            hand: Hand as AttemptFrom<Single, u8>
        }
    }
}

slice_serializable! {
    #[derive(Debug)]
    pub struct InteractEntity {
        pub entity_id: i32 as VarInt,
        pub mode: InteractMode,
        pub shift_key_down: bool as Single
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

slice_serializable! {
    #[derive(Debug)]
    pub struct MovePlayerOnGround {
        pub on_ground: bool as Single
    }
}

// Player Abilities
slice_serializable! {
    #[derive(Debug)]
    pub struct PlayerAbilities {
        pub flags: u8 as Single,
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

// Set Carried Item
slice_serializable! {
    #[derive(Debug)]
    pub struct SetCarriedItem {
        pub slot: u16 as BigEndian
    }
}

// Set Creative Mode Slot
slice_serializable! {
    #[derive(Debug)]
    pub struct SetCreativeModeSlot<'a> {
        pub slot: i16 as BigEndian,
        pub item: Option<ProtocolItemStack<'a>>
    }
}

// Swing
slice_serializable! {
    #[derive(Debug)]
    pub struct Swing {
        pub hand: Hand as AttemptFrom<Single, u8>
    }
}

// Use Item On
slice_serializable! {
    #[derive(Debug)]
    pub struct UseItemOn {
        pub hand: Hand as AttemptFrom<Single, u8>,
        pub block_hit: BlockHitResult,
        pub sequence: i32 as VarInt
    }
}

// Use Item
slice_serializable! {
    #[derive(Debug)]
    pub struct UseItem {
        pub hand: Hand as AttemptFrom<Single, u8>,
        pub sequence: i32 as VarInt
    }
}
