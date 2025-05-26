use graphite_binary::slice_serialization::*;
use graphite_mc_constants::types::*;

use crate::identify_packets;
use crate::types::hashed_stack::HashedStack;
use crate::types::BlockHitResult;
use crate::types::BlockPosition;
use crate::types::ChangedSlot;
use crate::types::ItemStack;
use crate::IdentifiedPacket;
use num_enum::TryFromPrimitive;

identify_packets! {
    PacketId,
    AcceptTeleportation = 0x00,
    // BlockEntityTagQuery = 0x01,
    BundleItemSelected = 0x02,
    // ChangeDifficulty = 0x03,
    // ChatAck = 0x04,
    ChatCommand<'_> = 0x05,
    // ChatCommandSigned = 0x06,
    // Chat = 0x07,
    // ChatSessionUpdate = 0x08
    // ChunkBatchReceived = 0x09,
    ClientAction = 0x0a,
    ClientTickEnd = 0x0b,
    ClientInformation<'_> = 0x0c,
    CommandSuggestion<'_> = 0x0d,
    AcknowledgeConfiguration = 0x0e,
    // ContainerButtonClick = 0x0f,
    ContainerClick = 0x10,
    ContainerClose = 0x11,
    // ContainerSlotStateChanged = 0x12,
    // CookieResponse = 0x13,
    CustomPayload<'_> = 0x14,
    // DebugSampleSubscription = 0x15,
    // EditBook = 0x16,
    // EntityTagQuery = 0x17,
    InteractEntity = 0x18,
    // JigsawGenerate = 0x19,
    KeepAlive = 0x1a,
    // LockDifficulty = 0x1b,
    MovePlayerPos = 0x1c,
    MovePlayerPosRot = 0x1d,
    MovePlayerRot = 0x1e,
    MovePlayerOnGround = 0x1f,
    // MoveVehicle = 0x20,
    // PaddleBoat = 0x21,
    // PickItemFromBlock = 0x22,
    // PickItemFromEntity = 0x23,
    // PingRequest = 0x24,
    PlaceRecipe = 0x25,
    PlayerAbilities = 0x26,
    PlayerHandAction = 0x27,
    PlayerMoveAction = 0x28,
    PlayerInput = 0x29,
    PlayerLoaded = 0x2a,
    Pong = 0x2b,
    // RecipeBookChangeSettings = 0x2c,
    // RecipeBookSeenRecipe = 0x2d,
    // RenameItem = 0x2e,
    // ResourcePack = 0x2f,
    // SeenAdvancements = 0x30,
    SelectTrade = 0x31,
    // SetBeaconEffect = 0x32,
    SetCarriedItem = 0x33,
    // SetCommandBlock = 0x34,
    // SetCommandBlockMinecart = 0x35,
    SetCreativeModeSlot = 0x36,
    // SetJigsawBlock = 0x37,
    // SetStructureBlock = 0x38,
    // SetTestBlock = 0x39,
    // UpdateSign = 0x3a,
    Swing = 0x3b,
    // TeleportToEntity = 0x3c,
    // TestInstanceBlockAction = 0x3d,
    UseItemOn = 0x3e,
    UseItem = 0x3f
}

// Accept Teleportation

slice_serializable! {
    #[derive(Debug)]
    pub struct AcceptTeleportation {
        pub id: i32 as VarInt
    }
}

slice_serializable! {
    #[derive(Debug)]
    pub struct BundleItemSelected {
        pub slot: i32 as VarInt,
        pub selected_index: i32 as VarInt,
    }
}

// Chat Command

slice_serializable! {
    #[derive(Debug)]
    pub struct Signature<'a> {
        pub string: &'a str as SizedString<16>,
        pub bytes: &'a [u8] as FixedBlob<256>
    }
}

slice_serializable! {
    #[derive(Debug)]
    pub struct ChatCommand<'a> {
        pub command: &'a str as SizedString<256>,
    }
}

slice_serializable! {
    #[derive(Debug)]
    pub struct ChatCommandSigned<'a> {
        pub command: &'a str as SizedString<256>,
        pub timestamp: u64 as BigEndian,
        pub salt: u64 as BigEndian,
        pub signatures: Vec<Signature<'a>> as SizedArray<Signature<'_>, 8>,

        pub ignored: &'a [u8] as GreedyBlob
    }
}

slice_serializable! {
    #[derive(Clone, Copy, Debug)]
    pub struct ClientAction {
        pub action: graphite_mc_constants::types::ClientAction as AttemptFrom<Single, u8>,
    }
}

slice_serializable! {
    #[derive(Clone, Copy, Debug)]
    pub struct ClientTickEnd;
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
        pub arm_position: HumanoidArm as AttemptFrom<Single, u8>,
        pub text_filtering_enabled: bool as Single,
        pub show_on_server_list: bool as Single,
        pub particle_status: ParticleStatus as AttemptFrom<Single, u8>
    }
}

slice_serializable! {
    #[derive(Debug)]
    pub struct CommandSuggestion<'a> {
        pub id: i32 as VarInt,
        pub command: &'a str as SizedString<256>,
    }
}

slice_serializable! {
    #[derive(Clone, Copy, Debug)]
    pub struct AcknowledgeConfiguration;
}

// Container Click
slice_serializable! {
    #[derive(Debug)]
    pub struct ContainerClick {
        pub container_id: i32 as VarInt,
        pub state_id: i32 as VarInt,
        pub slot: i16 as BigEndian,
        pub button: i8 as Single,
        pub mode: ClickType as AttemptFrom<Single, u8>,
        pub changed_slots: Vec<ChangedSlot> as SizedArray<ChangedSlot, 128>,
        pub carried_item: Option<HashedStack>
    }
}

// Container Close
slice_serializable! {
    #[derive(Debug)]
    pub struct ContainerClose {
        pub container_id: i32 as VarInt
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
            hand: InteractionHand as AttemptFrom<Single, u8>
        },
        Attack {
        },
        InteractAt {
            offset_x: f32 as BigEndian,
            offset_y: f32 as BigEndian,
            offset_z: f32 as BigEndian,
            hand: InteractionHand as AttemptFrom<Single, u8>
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
        pub on_ground: bool as packed!(),
        pub horizontal_collision: bool as packed!()
    }
}

slice_serializable! {
    #[derive(Debug)]
    pub struct MovePlayerRot {
        pub yaw: f32 as BigEndian,
        pub pitch: f32 as BigEndian,
        pub on_ground: bool as packed!(),
        pub horizontal_collision: bool as packed!()
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
        pub on_ground: bool as packed!(),
        pub horizontal_collision: bool as packed!()
    }
}

slice_serializable! {
    #[derive(Debug)]
    pub struct MovePlayerOnGround {
        pub on_ground: bool as packed!(),
        pub horizontal_collision: bool as packed!()
    }
}

slice_serializable! {
    #[derive(Debug)]
    pub struct PlaceRecipe {
        pub container_id: i32 as VarInt,
        pub recipe_id: i32 as VarInt,
        pub use_max_items: bool as Single
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

slice_serializable! {
    #[derive(Debug)]
    pub struct PlayerInput {
        pub forward: bool as packed!(),
        pub backward: bool as packed!(),
        pub left: bool as packed!(),
        pub right: bool as packed!(),
        pub jump: bool as packed!(),
        pub shift: bool as packed!(),
        pub sprint: bool as packed!(),
    }
}

slice_serializable! {
    #[derive(Clone, Copy, Debug)]
    pub struct PlayerLoaded;
}

slice_serializable! {
    #[derive(Debug)]
    pub struct Pong {
        pub id: i32 as BigEndian,
    }
}

// Select Trade
slice_serializable! {
    #[derive(Debug)]
    pub struct SelectTrade {
        pub trade: i32 as VarInt
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
    pub struct SetCreativeModeSlot {
        pub slot: i16 as BigEndian,
        pub item: ItemStack
    }
}

// Swing
slice_serializable! {
    #[derive(Debug)]
    pub struct Swing {
        pub hand: InteractionHand as AttemptFrom<Single, u8>
    }
}

// Use Item On
slice_serializable! {
    #[derive(Debug)]
    pub struct UseItemOn {
        pub hand: InteractionHand as AttemptFrom<Single, u8>,
        pub block_hit: BlockHitResult,
        pub sequence: i32 as VarInt
    }
}

// Use Item
slice_serializable! {
    #[derive(Debug)]
    pub struct UseItem {
        pub hand: InteractionHand as AttemptFrom<Single, u8>,
        pub sequence: i32 as VarInt,
        pub yaw: f32 as BigEndian,
        pub pitch: f32 as BigEndian,
    }
}
