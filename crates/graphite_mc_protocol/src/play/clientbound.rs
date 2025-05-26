use data_component::DataComponentPredicate;
use enumset::EnumSet;
use enumset::EnumSetType;
use graphite_binary::nbt::EncodedNBT;
use graphite_binary::slice_serialization::*;
use graphite_mc_constants::builtin::Attribute;
use graphite_mc_constants::builtin::RecipeBookCategory;
use graphite_mc_constants::builtin::SoundEvent;
use graphite_mc_constants::entity::Metadata;
use graphite_mc_constants::item::Item;
use graphite_mc_constants::particle::Particle;
use graphite_mc_constants::types::*;
use graphite_network::PacketBuffer;
use std::borrow::Cow;
use std::ops::Deref;
use std::ops::DerefMut;

use crate::identify_packets;
use crate::types::holder_set::HolderSet;
use crate::types::*;
use crate::IdentifiedPacket;
use num_enum::{IntoPrimitive, TryFromPrimitive};

identify_packets! {
    PlayPacket,
    BundleDelimiter = 0x00,
    AddEntity = 0x01,
    AnimateEntity = 0x02,
    // AwardStats = 0x03,
    BlockChangedAck = 0x04,
    BlockDestruction = 0x05,
    // BlockEntityData = 0x06,
    // BlockEvent = 0x07,
    BlockUpdate = 0x08,
    BossEvent = 0x09,
    // ChangeDifficulty = 0x0a,
    // ChunkBatchFinished = 0bx0b,
    // ChunkBatchStart = 0x0c,
    // ChunksBiomes = 0x0d,
    // ClearTitles = 0x0e,
    CommandSuggestions<'_> = 0x0f,
    Commands = 0x10,
    ContainerClose = 0x11,
    ContainerSetContent<'_> = 0x12,
    // ContainerSetData = 0x13,
    ContainerSetSlot = 0x14,
    // CookieRequest = 0x15,
    Cooldown<'_> = 0x16,
    // CustomChatCompletions = 0x17,
    CustomPayload<'_> = 0x18,
    DamageEvent = 0x19,
    // DebugSample = 0x1a,
    // DeleteChat = 0x1b,
    Disconnect = 0x1c,
    // DisguisedChat = 0x1d,
    EntityEvent = 0x1e,
    EntityPositionSync = 0x1f,
    // Explode = 0x20,
    // ForgetLevelChunk = 0x21,
    GameEvent = 0x22,
    // HorseScreenOpen = 0x23,
    HurtAnimation = 0x24,
    // InitializeBorder = 0x25,
    KeepAlive = 0x26,
    LevelChunkWithLight<'_> = 0x27,
    LevelEvent = 0x28,
    LevelParticles = 0x29,
    // LightUpdate = 0x2a,
    JoinGame<'_> = 0x2b,
    // MapItemData = 0x2c,
    MerchantOffers<'_> = 0x2d,
    MoveEntityPos = 0x2e,
    MoveEntityPosRot = 0x2f,
    // MoveMinecartAlongTrack = 0x30,
    MoveEntityRot = 0x31,
    // MoveVehicle = 0x32,
    // OpenBook = 0x33,
    OpenScreen = 0x34,
    // OpenSignEditor = 0x35,
    Ping = 0x36,
    // PongResponse = 0x37,
    PlaceGhostRecipe<'_> = 0x38,
    PlayerAbilities = 0x39,
    // PlayerChat = 0x3a,
    // PlayerCombatEnd = 0x3b,
    // PlayerCombatEnter = 0x3c,
    // PlayerCombatKill = 0x3d,
    PlayerInfoRemove = 0x3e,
    PlayerInfoUpdate<'_> = 0x3f,
    PlayerLookAt = 0x40,
    PlayerPosition = 0x41,
    // PlayerRotation = 0x42,
    RecipeBookAdd<'_> = 0x43,
    // RecipeBookRemove = 0x44,
    RecipeBookSettings = 0x45,
    RemoveEntities<'_> = 0x46,
    // RemoveMobEffect = 0x47,
    // ResetScore = 0x48,
    // ResourcePackPop = 0x49,
    // ResourcePackPush = 0x4a,
    Respawn<'_> = 0x4b,
    RotateHead = 0x4c,
    // SectionBlocksUpdate = 0x4d,
    // SelectAdvancementTab = 0x4e,
    // ServerData = 0x4f,
    // SetActionBarText = 0x50,
    // SetBorderCenter = 0x51,
    // SetBorderLerpSize = 0x52,
    // SetBorderSize = 0x53,
    // SetBorderWarningDelay = 0x54,
    // SetBorderWarningDistance = 0x55,
    SetCamera = 0x56,
    SetChunkCacheCenter = 0x57,
    // SetChunkCacheRadius = 0x58,
    SetCursorItem = 0x59,
    // SetDefaultSpawnPosition = 0x5a,
    SetDisplayObjective<'_> = 0x5b,
    SetEntityData<'_> = 0x5c,
    // SetEntityLink = 0x5d,
    SetEntityMotion = 0x5e,
    SetEquipment = 0x5f,
    SetExperience = 0x60,
    // SetHealth = 0x61,
    // SetHeldSlot = 0x62,
    SetObjective<'_> = 0x63,
    SetPassengers<'_> = 0x64,
    // SetPlayerInventory = 0x65,
    // SetPlayerTeam = 0x66,
    // SetScore = 0x67,
    // SetSimulationDistance = 0x68,
    SetSubtitleText = 0x69,
    SetTime = 0x6a,
    SetTitleText = 0x6b,
    SetTitleAnimation = 0x6c,
    SoundEntity<'_> = 0x6d,
    Sound<'_> = 0x6e,
    StartConfiguration = 0x6f,
    // StopSound = 0x70,
    // StoreCookie = 0x71,
    SystemChat = 0x72,
    // TabList = 0x73,
    // TagQuery = 0x74,
    TakeItemEntity = 0x75,
    TeleportEntity = 0x76,
    // TestInstanceBlockStatus = 0x77,
    // TickingState = 0x78,
    // TickingStep = 0x79,
    // Transfer = 0x7a,
    UpdateAdvancements<'_> = 0x7b,
    UpdateAttributes<'_> = 0x7c,
    // UpdateMobEffect = 0x7d,
    // UpdateRecipes = 0x7e,
    UpdateTags<'_> = 0x7f
    // ProjectilePower = 0x80
    // CustomReportDetails = 0x81
    // ServerLinks = 0x82
}

slice_serializable! {
    #[derive(Debug, Default, Clone, Copy)]
    struct BundleDelimiter;
}

pub struct BundledPacketBuffer<'a> {
    written_bundle: bool,
    buffer: &'a mut PacketBuffer
}

impl <'a> BundledPacketBuffer<'a> {
    pub fn new(buffer: &'a mut PacketBuffer) -> Self {
        Self {
            written_bundle: false,
            buffer
        }
    }

    pub fn write<'r, 'd: 'r, T: IdentifiedPacket<PlayPacket> + SliceSerializable<'r, 'd, T>>(&mut self, packet: &'r T) {
        if !self.written_bundle {
            self.written_bundle = true;
            self.buffer.write_serializable(BundleDelimiter.get_packet_id_as_u8(), &BundleDelimiter);
        }
        self.buffer.write_serializable(packet.get_packet_id_as_u8(), packet);
    }

    pub fn finish(self) {
        // Will call Drop code
    }
}

impl <'a> Deref for BundledPacketBuffer<'a> {
    type Target = PacketBuffer;

    fn deref(&self) -> &Self::Target {
        self.buffer
    }
}

impl <'a> DerefMut for BundledPacketBuffer<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        if !self.written_bundle {
            self.written_bundle = true;
            self.buffer.write_serializable(BundleDelimiter.get_packet_id_as_u8(), &BundleDelimiter);
        }
        self.buffer
    }
}

impl <'a> Drop for BundledPacketBuffer<'a> {
    fn drop(&mut self) {
        if self.written_bundle {
            self.buffer.write_serializable(BundleDelimiter.get_packet_id_as_u8(), &BundleDelimiter);
        }
    }
}


// Add Entity
slice_serializable! {
    #[derive(Debug, Default)]
    pub struct AddEntity {
        pub id: i32 as VarInt,
        pub uuid: u128 as BigEndian,
        pub entity_type: i32 as VarInt,
        pub x: f64 as BigEndian,
        pub y: f64 as BigEndian,
        pub z: f64 as BigEndian,
        pub pitch: f32 as ByteRotation,
        pub yaw: f32 as ByteRotation,
        pub head_yaw: f32 as ByteRotation,
        pub data: i32 as VarInt,
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

// Animate Entity
slice_serializable! {
    #[derive(Debug)]
    pub struct AnimateEntity {
        pub entity_id: i32 as VarInt,
        pub animation: graphite_mc_constants::types::EntityAnimation as AttemptFrom<Single, u8>
    }
}

// Block Changed Ack
slice_serializable! {
    #[derive(Debug)]
    pub struct BlockChangedAck {
        pub sequence: i32 as VarInt
    }
}

// Block Destruction
slice_serializable! {
    #[derive(Debug)]
    pub struct BlockDestruction {
        pub entity_id: i32 as VarInt,
        pub location: BlockPosition,
        pub destroy_stage: i8 as Single
    }
}

// Block Update
slice_serializable! {
    #[derive(Debug)]
    pub struct BlockUpdate {
        pub pos: BlockPosition,
        pub block_state: i32 as VarInt
    }
}

// Boss Event
slice_serializable! {
    #[derive(Debug)]
    pub enum BossEventAction {
        Add {
            title: EncodedNBT as NBTBlob,
            health: f32 as BigEndian,
            color: BossBarColor as AttemptFrom<Single, u8>,
            division: BossBarOverlay as AttemptFrom<Single, u8>,
            flags: u8 as Single
        },
        Remove {}
    }
}

slice_serializable! {
    #[derive(Debug)]
    pub struct BossEvent {
        pub uuid: u128 as BigEndian,
        pub action: BossEventAction
    }
}

// Command Suggestions
slice_serializable! {
    #[derive(Debug)]
    pub struct CommandSuggestionEntry<'a> {
        pub text: Cow<'a, str> as SizedString,
        pub tooltip: Option<EncodedNBT> as Option<NBTBlob>
    }
}

slice_serializable! {
    #[derive(Debug)]
    pub struct CommandSuggestions<'a> {
        pub id: i32 as VarInt,
        pub start: i32 as VarInt,
        pub length: i32 as VarInt,
        pub entries: Vec<CommandSuggestionEntry<'a>> as SizedArray<CommandSuggestionEntry<'a>>
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

// Container Close
slice_serializable! {
    #[derive(Debug)]
    pub struct ContainerClose {
        pub container_id: i32 as VarInt,
    }
}

// Container Set Content
slice_serializable! {
    #[derive(Debug)]
    pub struct ContainerSetContent<'a> {
        pub container_id: i32 as VarInt,
        pub state_id: i32 as VarInt,
        pub slots: Cow<'a, [ItemStack]> as SizedArray<ItemStack>,
        pub carried: ItemStack
    }
}

// Container Set Slot
slice_serializable! {
    #[derive(Debug)]
    pub struct ContainerSetSlot {
        pub container_id: i32 as VarInt,
        pub state_id: i32 as VarInt,
        pub slot: i16 as BigEndian,
        pub item: ItemStack
    }
}

// Commands
slice_serializable! {
    #[derive(Debug)]
    pub struct Cooldown<'a> {
        pub group: Cow<'a, str> as SizedString<256>,
        pub cooldown: i32 as VarInt
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

// Damage Event
slice_serializable! {
    #[derive(Debug)]
    pub struct DamageEvent {
        pub entity_id: i32 as VarInt,
        pub source_type_id: i32 as VarInt,
        pub source_cause_id: i32 as VarInt,
        pub source_direct_id: i32 as VarInt,
        pub source_position: Option<Position>,
    }
}

// Disconnect
slice_serializable! {
    #[derive(Debug)]
    pub struct Disconnect {
        pub message: EncodedNBT as NBTBlob
    }
}

// Entity Event
slice_serializable! {
    #[derive(Debug)]
    pub struct EntityEvent {
        pub entity_id: i32 as BigEndian,
        pub status: u8 as Single,
    }
}

// Entity Position Sync
slice_serializable! {
    #[derive(Debug, Default)]
    pub struct EntityPositionSync {
        pub entity_id: i32 as VarInt,
        pub x: f64 as BigEndian,
        pub y: f64 as BigEndian,
        pub z: f64 as BigEndian,
        pub x_vel: f64 as BigEndian,
        pub y_vel: f64 as BigEndian,
        pub z_vel: f64 as BigEndian,
        pub yaw: f32 as BigEndian,
        pub pitch: f32 as BigEndian,
        pub on_ground: bool as Single
    }
}

// Forget Level Chunk
slice_serializable! {
    #[derive(Debug)]
    pub struct ForgetLevelChunk {
        pub chunk_x: i32 as BigEndian,
        pub chunk_z: i32 as BigEndian
    }
}

// Game Event

#[derive(Debug, Copy, Clone, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum GameEventType {
    NoRespawnBlockAvailable,
    StartRaining,
    StopRaining,
    ChangeGameMode,
    WinGame,
    DemoEvent,
    ArrowHitPlayer,
    RainLevelChange,
    ThunderLevelChange,
    PufferFishSting,
    GuardianElderEffect,
    ImmediateRespawn,
    LimitedCrafting,
    StartWaitingForLevelChunks,
}

slice_serializable! {
    #[derive(Debug)]
    pub struct GameEvent {
        pub event_type: GameEventType as AttemptFrom<Single, u8>,
        pub param: f32 as BigEndian
    }
}

// Initialize World Border
slice_serializable! {
    #[derive(Debug)]
    pub struct InitializeBorder {
        pub x: f64 as BigEndian,
        pub z: f64 as BigEndian,
        pub old_diameter: f64 as BigEndian,
        pub new_diameter: f64 as BigEndian,
        pub speed: i64 as VarInt,
        pub portal_teleport_boundary: i32 as VarInt,
        pub warning_blocks: i32 as VarInt,
        pub warning_time: i32 as VarInt,
    }
}

// Hurt Animation
slice_serializable! {
    #[derive(Debug)]
    pub struct HurtAnimation {
        pub entity_id: i32 as VarInt,
        pub hurt_angle: f32 as BigEndian
    }
}

// Keep Alive
slice_serializable! {
    #[derive(Debug)]
    pub struct KeepAlive {
        pub id: u64 as BigEndian
    }
}

// Level Event
slice_serializable! {
    #[derive(Debug)]
    pub struct LevelEvent {
        pub event_type: graphite_mc_constants::types::LevelEvent as AttemptFrom<BigEndian, i32>,
        pub pos: BlockPosition,
        pub data: i32 as BigEndian,
        pub global: bool as Single // global used by vanilla for dragon death and portal opening
    }
}

// Level Particles
slice_serializable! {
    #[derive(Debug)]
    pub struct LevelParticles {
        pub override_limiter: bool as Single,
        pub always_show: bool as Single,
        pub x: f64 as BigEndian,
        pub y: f64 as BigEndian,
        pub z: f64 as BigEndian,
        pub offset_x: f32 as BigEndian,
        pub offset_y: f32 as BigEndian,
        pub offset_z: f32 as BigEndian,
        pub max_speed: f32 as BigEndian,
        pub particle_count: i32 as BigEndian,
        pub particle: Particle
    }
}


// LevelChunkWithLight
slice_serializable! {
    #[derive(Debug)]
    pub struct LevelChunkWithLight<'a> {
        pub chunk_x: i32 as BigEndian,
        pub chunk_z: i32 as BigEndian,
        pub chunk_block_data: ChunkBlockData<'a>,
        pub chunk_light_data: ChunkLightData<'a>
    }
}

slice_serializable! {
    #[derive(Debug, Clone)]
    pub struct HeightmapEntry<'a> {
        pub id: u8 as Single,
        pub data: Cow<'a, [u64]> as SizedArray<BigEndian>
    }
}

slice_serializable! {
    #[derive(Debug)]
    pub struct ChunkBlockData<'a> {
        pub heightmaps: Cow<'a, [HeightmapEntry<'a>]> as SizedArray<HeightmapEntry<'a>>,
        pub data: &'a [u8] as SizedBlob,
        pub block_entity_count: i32 as VarInt,
        pub block_entity_data: &'a [u8] as WriteOnlyBlob
    }
}

slice_serializable! {
    #[derive(Debug)]
    pub struct ChunkLightData<'a> {
        pub sky_light_mask: Vec<u64> as SizedArray<BigEndian>,
        pub block_light_mask: Vec<u64> as SizedArray<BigEndian>,
        pub empty_sky_light_mask: Vec<u64> as SizedArray<BigEndian>,
        pub empty_block_light_mask: Vec<u64> as SizedArray<BigEndian>,
        pub sky_light_entries: Vec<Cow<'a, [u8]>> as SizedArray<SizedBlob>,
        pub block_light_entries: Vec<Cow<'a, [u8]>> as SizedArray<SizedBlob>
    }
}

// Join game
slice_serializable! {
    #[derive(Debug)]
    pub struct JoinGame<'a> {
        pub entity_id: i32 as BigEndian,
        pub is_hardcore: bool as Single,
        pub dimension_names: Vec<&'a str> as SizedArray<SizedString>,
        pub max_players: i32 as VarInt,
        pub view_distance: i32 as VarInt,
        pub simulation_distance: i32 as VarInt,
        pub reduced_debug_info: bool as Single,
        pub enable_respawn_screen: bool as Single,
        pub do_limited_crafting: bool as Single,
        pub dimension_type: i32 as VarInt,
        pub dimension_name: &'a str as SizedString,
        pub hashed_seed: u64 as BigEndian,
        pub gamemode: u8 as Single,
        pub previous_gamemode: i8 as Single,
        pub is_debug: bool as Single,
        pub is_flat: bool as Single,
        pub death_location: Option<GlobalPosition>,
        pub portal_cooldown: i32 as VarInt,
        pub sea_level: i32 as VarInt,
        pub enforces_secure_chat: bool as Single
    }
}

// Merchant Offers

slice_serializable! {
    #[derive(Debug)]
    pub struct MerchantOffers<'a> {
        pub container_id: i32 as VarInt,
        pub trades: Cow<'a, [MerchantTrade]> as SizedArray<MerchantTrade>,
        pub villager_level: i32 as VarInt,
        pub experience: i32 as VarInt,
        pub is_regular_villager: bool as Single,
        pub can_restock: bool as Single
    }
}

slice_serializable! {
    #[derive(Clone, Debug)]
    pub struct MerchantTrade {
        pub input: ItemCost,
        pub output: ItemStack,
        pub secondary_input: Option<ItemCost>,
        pub trade_disabled: bool as Single,
        pub trade_uses: i32 as BigEndian,
        pub max_trade_uses: i32 as BigEndian,
        pub experience: i32 as BigEndian,
        pub special_price: i32 as BigEndian,
        pub price_multiplier: f32 as BigEndian,
        pub demand: i32 as BigEndian,
    }
}

slice_serializable! {
    #[derive(Clone, Debug)]
    pub struct ItemCost {
        pub item: Item as AttemptFrom<VarInt, u16>,
        pub count: i32 as VarInt,
        pub predicate: DataComponentPredicate

    }
}

// Move Entity

slice_serializable! {
    #[derive(Debug)]
    pub struct MoveEntityPos {
        pub entity_id: i32 as VarInt,
        pub delta_x: i16 as BigEndian,
        pub delta_y: i16 as BigEndian,
        pub delta_z: i16 as BigEndian,
        pub on_ground: bool as Single,
    }
}

slice_serializable! {
    #[derive(Debug)]
    pub struct MoveEntityPosRot {
        pub entity_id: i32 as VarInt,
        pub delta_x: i16 as BigEndian,
        pub delta_y: i16 as BigEndian,
        pub delta_z: i16 as BigEndian,
        pub yaw: f32 as ByteRotation,
        pub pitch: f32 as ByteRotation,
        pub on_ground: bool as Single,
    }
}

slice_serializable! {
    #[derive(Debug)]
    pub struct MoveEntityRot {
        pub entity_id: i32 as VarInt,
        pub yaw: f32 as ByteRotation,
        pub pitch: f32 as ByteRotation,
        pub on_ground: bool as Single,
    }
}

slice_serializable! {
    #[derive(Debug)]
    pub struct OpenScreen {
        pub container_id: i32 as VarInt,
        pub screen_type: i32 as VarInt,
        pub title: EncodedNBT as NBTBlob
    }
}

slice_serializable! {
    #[derive(Debug)]
    pub struct Ping {
        pub id: i32 as BigEndian,
    }
}

slice_serializable! {
    #[derive(Debug)]
    pub struct PlaceGhostRecipe<'a> {
        pub container_id: i32 as VarInt,
        pub recipe_display: Cow<'a, RecipeDisplay<'a>>
    }
}


// Player Abilities
slice_serializable! {
    #[derive(Debug)]
    pub struct PlayerAbilities {
        pub invulnerable: bool as packed!(),
        pub is_flying: bool as packed!(),
        pub allow_flying: bool as packed!(),
        pub instant_breaking: bool as packed!(),
        pub flying_speed: f32 as BigEndian,
        pub walking_speed: f32 as BigEndian,
    }
}

// Remove Entities
slice_serializable! {
    #[derive(Debug)]
    pub struct RemoveEntities<'a> {
        pub entities: Cow<'a, [i32]> as SizedArray<VarInt>
    }
}

// PlayerInfo
#[derive(Debug)]
pub struct PlayerInfoEntry<'a> {
    pub profile: GameProfile<'a>,
    pub listed: bool,
    pub latency: i32,
    pub gamemode: u8,
    pub display_name: Option<EncodedNBT>
}

#[derive(EnumSetType, Debug)]
pub enum PlayerInfoAction {
    AddPlayer,
    InitializeChat,
    UpdateGameMode,
    UpdateListed,
    UpdateLatency,
    UpdateDisplayName
}

slice_serializable! {
    #[derive(Debug)]
    pub struct PlayerInfoRemove {
        pub players: Vec<u128> as SizedArray<BigEndian>
    }
}

#[derive(Debug)]
pub struct PlayerInfoUpdate<'a> {
    pub actions: EnumSet<PlayerInfoAction>,
    pub entries: Vec<PlayerInfoEntry<'a>>
}

impl <'r, 'd: 'r> SliceSerializable<'r, 'd, Self> for PlayerInfoUpdate<'d> {
    type CopyType = &'r Self;

    fn as_copy_type(t: &'r Self) -> Self::CopyType {
        t
    }

    fn read(_: &mut &'d [u8]) -> anyhow::Result<Self> {
        unimplemented!()
    }

    unsafe fn write(mut bytes: &mut [u8], data: Self::CopyType) -> &mut [u8] {
        bytes = <Single as SliceSerializable<u8>>::write(bytes, data.actions.as_u8());
        bytes = <VarInt as SliceSerializable<i32>>::write(bytes, data.entries.len() as i32);
        for entry in &data.entries {
            if data.actions.contains(PlayerInfoAction::AddPlayer) {
                bytes = GameProfile::write(bytes, &entry.profile);
            } else {
                bytes = <BigEndian as SliceSerializable<u128>>::write(bytes, entry.profile.uuid);
            }
            if data.actions.contains(PlayerInfoAction::InitializeChat) {
                unimplemented!()
            }
            if data.actions.contains(PlayerInfoAction::UpdateGameMode) {
                bytes = <Single as SliceSerializable<u8>>::write(bytes, entry.gamemode);
            }
            if data.actions.contains(PlayerInfoAction::UpdateListed) {
                bytes = <Single as SliceSerializable<bool>>::write(bytes, entry.listed);
            }
            if data.actions.contains(PlayerInfoAction::UpdateLatency) {
                bytes = <VarInt as SliceSerializable<i32>>::write(bytes, entry.latency);
            }
            if data.actions.contains(PlayerInfoAction::UpdateDisplayName) {
                if let Some(display_name) = entry.display_name.as_ref() {
                    bytes = <Single as SliceSerializable<bool>>::write(bytes, true);
                    bytes = <NBTBlob as SliceSerializable<EncodedNBT>>::write(bytes, display_name);
                } else {
                    bytes = <Single as SliceSerializable<bool>>::write(bytes, false);
                }
            }
        }
        bytes
    }

    fn get_write_size(data: Self::CopyType) -> usize {
        let mut size = <Single as SliceSerializable<u8>>::get_write_size(data.actions.as_u8());
        size += <VarInt as SliceSerializable<i32>>::get_write_size(data.entries.len() as i32);
        for entry in &data.entries {
            if data.actions.contains(PlayerInfoAction::AddPlayer) {
                size += GameProfile::get_write_size(&entry.profile);
            } else {
                size += <BigEndian as SliceSerializable<u128>>::get_write_size(entry.profile.uuid);
            }
            if data.actions.contains(PlayerInfoAction::InitializeChat) {
                unimplemented!()
            }
            if data.actions.contains(PlayerInfoAction::UpdateGameMode) {
                size += <Single as SliceSerializable<u8>>::get_write_size(entry.gamemode);
            }
            if data.actions.contains(PlayerInfoAction::UpdateListed) {
                size += <Single as SliceSerializable<bool>>::get_write_size(entry.listed);
            }
            if data.actions.contains(PlayerInfoAction::UpdateLatency) {
                size += <VarInt as SliceSerializable<i32>>::get_write_size(entry.latency);
            }
            if data.actions.contains(PlayerInfoAction::UpdateDisplayName) {
                if let Some(display_name) = entry.display_name.as_ref() {
                    size += <Single as SliceSerializable<bool>>::get_write_size(true);
                    size += <NBTBlob as SliceSerializable<EncodedNBT>>::get_write_size(display_name);
                } else {
                    size += <Single as SliceSerializable<bool>>::get_write_size(false);
                }
            }
        }
        size
    }
}

// Player Look At
slice_serializable! {
    #[derive(Debug)]
    pub struct LookAtEntity {
        pub entity: i32 as VarInt,
        pub anchor: u8 as Single
    }
}

slice_serializable! {
    #[derive(Debug)]
    pub struct PlayerLookAt {
        pub anchor: u8 as Single,
        pub x: f64 as BigEndian,
        pub y: f64 as BigEndian,
        pub z: f64 as BigEndian,
        pub target: Option<LookAtEntity>
    }
}

// Player Position
slice_serializable! {
    #[derive(Debug)]
    pub struct PlayerPosition {
        pub teleport_id: i32 as VarInt,
        pub x: f64 as BigEndian,
        pub y: f64 as BigEndian,
        pub z: f64 as BigEndian,
        pub x_vel: f64 as BigEndian,
        pub y_vel: f64 as BigEndian,
        pub z_vel: f64 as BigEndian,
        pub yaw: f32 as BigEndian,
        pub pitch: f32 as BigEndian,
        pub relative_arguments: i32 as BigEndian,
    }
}

// Recipe Book Add
slice_serializable! {
    #[derive(Debug)]
    pub struct RecipeBookAdd<'a> {
        pub recipes: Cow<'a, [RecipeBookAddEntry<'a>]> as SizedArray<RecipeBookAddEntry<'a>>,
        pub replace: bool as Single
    }
}

slice_serializable! {
    #[derive(Debug, Clone)]
    pub struct RecipeBookAddEntry<'a> {
        pub contents: RecipeDisplayEntry<'a>,
        pub notify: bool as packed!(),
        pub highlight: bool as packed!(),
    }
}

slice_serializable! {
    #[derive(Debug, Clone)]
    pub struct RecipeDisplayEntry<'a> {
        pub id: i32 as VarInt,
        pub display: Cow<'a, RecipeDisplay<'a>>,
        pub group: Option<i32> as OptionalVarInt,
        pub category: RecipeBookCategory as AttemptFrom<Single, u8>,
        pub crafting_requirements: Option<Cow<'a, [HolderSet<'a, Item>]>> as Option<SizedArray<HolderSet<'a, AttemptFrom<VarInt, u16>>>>
    }
}

slice_serializable! {
    #[derive(Debug, Clone)]
    pub enum RecipeDisplay<'a> {
        CraftingShapeless {
            ingredients: Cow<'a, [SlotDisplay]> as SizedArray<SlotDisplay>,
            result: SlotDisplay,
            crafting_station: SlotDisplay,
        },
        CraftingShaped {
            width: i32 as VarInt,
            height: i32 as VarInt,
            ingredients: Cow<'a, [SlotDisplay]> as SizedArray<SlotDisplay>,
            result: SlotDisplay,
            crafting_station: SlotDisplay,
        }
        // other recipe displays are omitted
    }
}

slice_serializable! {
    #[derive(Debug, Clone)]
    pub enum SlotDisplay {
        Empty,
        AnyFuel,
        Item {
            item: Item as AttemptFrom<VarInt, u16>
        },
        ItemStack {
            item_stack: ItemStack,
        },
        // other slot displays are omitted
    }
}


// Recipe Settings
slice_serializable! {
    #[derive(Debug)]
    pub struct RecipeBookSettings {
        pub crafting_open: bool as Single,
        pub crafting_filtering: bool as Single,
        pub furnace_open: bool as Single,
        pub furnace_filtering: bool as Single,
        pub blast_furnace_open: bool as Single,
        pub blast_furnace_filtering: bool as Single,
        pub smoker_open: bool as Single,
        pub smoker_filtering: bool as Single,
    }
}

// Respawn
slice_serializable! {
    #[derive(Debug)]
    pub struct Respawn<'a> {
        pub dimension_type: i32 as VarInt,
        pub dimension_name: &'a str as SizedString,
        pub hashed_seed: u64 as BigEndian,
        pub gamemode: u8 as Single,
        pub previous_gamemode: i8 as Single,
        pub is_debug: bool as Single,
        pub is_flat: bool as Single,
        pub death_location: Option<GlobalPosition>,
        pub portal_cooldown: i32 as VarInt,
        pub sea_level: i32 as VarInt,
        pub data_to_keep: u8 as Single
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

// Set Camera
slice_serializable! {
    #[derive(Debug)]
    pub struct SetCamera {
        pub entity_id: i32 as VarInt
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

// Set Cursor Item
slice_serializable! {
    #[derive(Debug)]
    pub struct SetCursorItem {
        pub item: ItemStack
    }
}

// Set Entity Motion
slice_serializable! {
    #[derive(Debug)]
    pub struct SetEntityMotion {
        pub entity_id: i32 as VarInt,
        pub x_vel: f32 as QuantizedShort,
        pub y_vel: f32 as QuantizedShort,
        pub z_vel: f32 as QuantizedShort,
    }
}

slice_serializable! {
    #[derive(Debug)]
    pub enum NumberFormat {
        Blank,
        Styled {
            style: EncodedNBT as NBTBlob,
        },
        Fixed {
            text: EncodedNBT as NBTBlob,
        }
    }
}


slice_serializable! {
    #[derive(Debug)]
    pub enum SetObjectiveMethod {
        Add {
            display: EncodedNBT as NBTBlob,
            render_type: ObjectiveRenderType as AttemptFrom<Single, u8>,
            number_format: Option<NumberFormat>
        },
        Remove,
        Change {
            display: EncodedNBT as NBTBlob,
            render_type: ObjectiveRenderType as AttemptFrom<Single, u8>,
            number_format: Option<NumberFormat>
        }
    }
}

// Set Objective
slice_serializable! {
    #[derive(Debug)]
    pub struct SetObjective<'a> {
        pub objective_name: Cow<'a, str> as SizedString,
        pub method: SetObjectiveMethod
    }
}

// Set Passengers
slice_serializable! {
    #[derive(Debug)]
    pub struct SetPassengers<'a> {
        pub entity_id: i32 as VarInt,
        pub passengers: Cow<'a, [i32]> as SizedArray<VarInt>,
    }
}

// Set Equipment
slice_serializable! {
    #[derive(Debug)]
    pub struct SetEquipment {
        pub entity_id: i32 as VarInt,
        pub equipment: Vec<(EquipmentSlot, ItemStack)> as EquipmentList
    }
}

// Set Experience
slice_serializable! {
    #[derive(Debug)]
    pub struct SetExperience {
        pub progress: f32 as BigEndian,
        pub level: i32 as VarInt,
        pub total: i32 as VarInt,
    }
}

// Set Display Objective
slice_serializable! {
    #[derive(Debug)]
    pub struct SetDisplayObjective<'a> {
        pub slot: DisplaySlot as AttemptFrom<Single, u8>,
        pub objective_name: Cow<'a, str> as SizedString
    }
}

// Set Entity Data
slice_serializable! {
    #[derive(Debug)]
    pub struct SetEntityData<'a> {
        pub entity_id: i32 as VarInt,
        pub data: &'a [u8] as GreedyBlob
    }
}

impl <'a> SetEntityData<'a> {
    pub fn write_changes<M: Metadata>(metadata: &mut M, entity_id: i32, buffer: &mut graphite_network::PacketBuffer) {
        Self::write_changes_without_clearing(metadata, entity_id, buffer);
        metadata.clear_all_changes();
    }

    pub fn write_changes_without_clearing<M: Metadata>(metadata: &mut M, entity_id: i32, buffer: &mut graphite_network::PacketBuffer) {
        let metadata_size = metadata.get_changes_write_size();
        if metadata_size == 0 {
            return;
        }

        let expected_packet_size = 16 + metadata_size;
        buffer.write_custom(Self::ID as u8, expected_packet_size, |mut bytes| {
            unsafe {
                bytes = <VarInt as SliceSerializable<i32>>::write(bytes, entity_id);
                bytes = metadata.write_changes(bytes);
            }
            bytes
        });
    }

    pub fn write_non_default<M: Metadata>(metadata: &M, entity_id: i32, buffer: &mut graphite_network::PacketBuffer) {
        let metadata_size = metadata.get_non_default_write_size();
        if metadata_size == 0 {
            return;
        }

        let expected_packet_size = 16 + metadata_size;
        buffer.write_custom(Self::ID as u8, expected_packet_size, |mut bytes| {
            unsafe {
                bytes = <VarInt as SliceSerializable<i32>>::write(bytes, entity_id);
                bytes = metadata.write_non_default(bytes);
            }
            bytes
        })
    }
}

// Set Subtitle
slice_serializable! {
    #[derive(Debug)]
    pub struct SetSubtitleText {
        pub text: EncodedNBT as NBTBlob
    }
}

// Set Time
slice_serializable! {
    #[derive(Debug)]
    pub struct SetTime {
        pub game_time: i64 as BigEndian,
        pub day_time: i64 as BigEndian,
        pub tick_day_time: bool as Single
    }
}

// Set Title
slice_serializable! {
    #[derive(Debug)]
    pub struct SetTitleText {
        pub text: EncodedNBT as NBTBlob
    }
}

// Set Title Animation
slice_serializable! {
    #[derive(Debug)]
    pub struct SetTitleAnimation {
        pub fade_in: i32 as BigEndian,
        pub stay: i32 as BigEndian,
        pub fade_out: i32 as BigEndian
    }
}

// Sound Entity
slice_serializable! {
    #[derive(Debug)]
    pub struct SoundEntity<'a> {
        pub sound: SoundType<'a>,
        pub source: SoundSource as AttemptFrom<Single, u8>,
        pub entity_id: i32 as VarInt,
        pub volume: f32 as BigEndian,
        pub pitch: f32 as BigEndian,
        pub seed: u64 as BigEndian,
    }
}

// Sound
slice_serializable! {
    #[derive(Debug)]
    pub struct Sound<'a> {
        pub sound: SoundType<'a>,
        pub source: SoundSource as AttemptFrom<Single, u8>,
        pub x: f32 as QuantizedInt<8>,
        pub y: f32 as QuantizedInt<8>,
        pub z: f32 as QuantizedInt<8>,
        pub volume: f32 as BigEndian,
        pub pitch: f32 as BigEndian,
        pub seed: u64 as BigEndian,
    }
}

slice_serializable! {
    #[derive(Copy, Clone, Debug)]
    pub struct StartConfiguration;
}

// System Chat
slice_serializable! {
    #[derive(Debug)]
    pub struct SystemChat {
        pub message: EncodedNBT as NBTBlob,
        pub overlay: bool as Single
    }
}

// Teleport Entity
slice_serializable! {
    #[derive(Debug)]
    pub struct TakeItemEntity {
        pub item_id: i32 as VarInt,
        pub player_id: i32 as VarInt,
        pub amount: i32 as VarInt,
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
        pub x_vel: f64 as BigEndian,
        pub y_vel: f64 as BigEndian,
        pub z_vel: f64 as BigEndian,
        pub yaw: f32 as BigEndian,
        pub pitch: f32 as BigEndian,
        pub relative_arguments: i32 as BigEndian,
        pub on_ground: bool as Single
    }
}

// Update Advancement

#[derive(Debug, Clone)]
pub struct AdvancementDisplayInfoFlags<'a> {
    pub background: Option<Cow<'a, str>>,
    pub show_toast: bool,
    pub hidden: bool
}

impl <'r, 'd: 'r> SliceSerializable<'r, 'd> for AdvancementDisplayInfoFlags<'d> {
    type CopyType = &'r Self;

    fn as_copy_type(t: &'r Self) -> Self::CopyType {
        t
    }

    fn read(bytes: &mut &'d [u8]) -> anyhow::Result<Self> {
        let flags: i32 = BigEndian::read(bytes)?;
        let background = if (flags & 1) != 0 {
            Some(SizedString::<32768>::read(bytes)?)
        } else {
            None
        };

        Ok(Self {
            background,
            show_toast: (flags & 2) != 0,
            hidden: (flags & 4) != 0,
        })
    }

    unsafe fn write(mut bytes: &mut [u8], data: Self::CopyType) -> &mut [u8] {
        let mut flags = 0;
        if data.background.is_some() {
            flags |= 1;
        }
        if data.show_toast {
            flags |= 2;
        }
        if data.hidden {
            flags |= 4;
        }
        bytes = <BigEndian as SliceSerializable<i32>>::write(bytes, flags);
        if let Some(background) = data.background.as_ref() {
            <SizedString as SliceSerializable<&'d str>>::write(bytes, background)
        } else {
            bytes
        }
    }

    fn get_write_size(data: Self::CopyType) -> usize {
        let mut size = <BigEndian as SliceSerializable<i32>>::get_write_size(7);
        if let Some(background) = data.background.as_ref() {
            size += <SizedString as SliceSerializable<&'d str>>::get_write_size(background);
        }
        size
    }
}

slice_serializable! {
    #[derive(Debug, Clone)]
    pub struct AdvancementDisplayInfo<'a> {
        pub title: EncodedNBT as NBTBlob,
        pub description: EncodedNBT as NBTBlob,
        pub icon: ItemStack,
        pub advancement_type: AdvancementType as AttemptFrom<Single, u8>,
        pub flags: AdvancementDisplayInfoFlags<'a>,
        pub x: f32 as BigEndian,
        pub y: f32 as BigEndian,
    }
}

#[derive(Debug, Clone)]
pub enum AdvancementRequirements<'a> {
    None,
    Single(Cow<'a, str>),
    AnyOf(Vec<Cow<'a, str>>),
    AllOf(Vec<Cow<'a, str>>),
    AllOfAnyOf(Vec<Vec<Cow<'a, str>>>),
}

impl <'r, 'd: 'r> SliceSerializable<'r, 'd> for AdvancementRequirements<'d> {
    type CopyType = &'r Self;

    fn as_copy_type(t: &'r Self) -> Self::CopyType {
        t
    }

    fn read(bytes: &mut &'d [u8]) -> anyhow::Result<Self> {
        let all_count: i32 = VarInt::read(bytes)?;

        if all_count == 1 {
            let any_count: i32 = VarInt::read(bytes)?;

            if any_count == 1 {
                Ok(Self::Single(SizedString::<32767>::read(bytes)?))
            } else {
                let mut any = Vec::with_capacity((any_count as usize).min(32));
                for _ in 0..any_count {
                    any.push(SizedString::<32767>::read(bytes)?);
                }
                Ok(Self::AnyOf(any))
            }
        } else {
            let mut all = Vec::with_capacity((all_count as usize).min(32));

            for _ in 0..all_count {
                let any_count: i32 = VarInt::read(bytes)?;

                let mut any = Vec::with_capacity((any_count as usize).min(32));
                for _ in 0..any_count {
                    any.push(SizedString::<32767>::read(bytes)?);
                }
                all.push(any);
            }

            Ok(Self::AllOfAnyOf(all))
        }
    }

    unsafe fn write(mut bytes: &mut [u8], data: Self::CopyType) -> &mut [u8] {
        match data {
            Self::None => {
                bytes = <Single as SliceSerializable<u8>>::write(bytes, 0);
                bytes
            },
            Self::Single(single) => {
                bytes = <Single as SliceSerializable<u8>>::write(bytes, 1);
                bytes = <Single as SliceSerializable<u8>>::write(bytes, 1);
                bytes = <SizedString as SliceSerializable<&'d str>>::write(bytes, single.as_ref());
                bytes
            },
            Self::AnyOf(any) => {
                bytes = <Single as SliceSerializable<u8>>::write(bytes, 1);
                bytes = <VarInt as SliceSerializable<i32>>::write(bytes, any.len() as i32);
                for value in any {
                    bytes = <SizedString as SliceSerializable<&'d str>>::write(bytes, value.as_ref());
                }
                bytes
            },
            Self::AllOf(all) => {
                bytes = <VarInt as SliceSerializable<i32>>::write(bytes, all.len() as i32);
                for value in all {
                    bytes = <Single as SliceSerializable<u8>>::write(bytes, 1);
                    bytes = <SizedString as SliceSerializable<&'d str>>::write(bytes, value.as_ref());
                }
                bytes
            },
            Self::AllOfAnyOf(all_any) => {
                bytes = <VarInt as SliceSerializable<i32>>::write(bytes, all_any.len() as i32);
                for any in all_any {
                    bytes = <VarInt as SliceSerializable<i32>>::write(bytes, any.len() as i32);
                    for value in any {
                        bytes = <SizedString as SliceSerializable<&'d str>>::write(bytes, value.as_ref());
                    }
                }
                bytes
            },
        }
    }

    fn get_write_size(data: Self::CopyType) -> usize {
        match data {
            Self::None => {
                <Single as SliceSerializable<u8>>::get_write_size(0)
            },
            Self::Single(single) => {
                let mut size = <Single as SliceSerializable<u8>>::get_write_size(1);
                size += <Single as SliceSerializable<u8>>::get_write_size(1);
                size += <SizedString as SliceSerializable<&'d str>>::get_write_size(single.as_ref());
                size
            },
            Self::AnyOf(any) => {
                let mut size = <Single as SliceSerializable<u8>>::get_write_size(1);
                size += <VarInt as SliceSerializable<i32>>::get_write_size(any.len() as i32);
                for value in any {
                    size += <SizedString as SliceSerializable<&'d str>>::get_write_size(value.as_ref());
                }
                size
            },
            Self::AllOf(all) => {
                let mut size = <VarInt as SliceSerializable<i32>>::get_write_size(all.len() as i32);
                for value in all {
                    size += <Single as SliceSerializable<u8>>::get_write_size(1);
                    size += <SizedString as SliceSerializable<&'d str>>::get_write_size(value.as_ref());
                }
                size
            },
            Self::AllOfAnyOf(all_any) => {
                let mut size = <VarInt as SliceSerializable<i32>>::get_write_size(all_any.len() as i32);
                for any in all_any {
                    size += <VarInt as SliceSerializable<i32>>::get_write_size(any.len() as i32);
                    for value in any {
                        size += <SizedString as SliceSerializable<&'d str>>::get_write_size(value.as_ref());
                    }
                }
                size
            },
        }
    }
}

slice_serializable! {
    #[derive(Debug, Clone)]
    pub struct Advancement<'a> {
        pub parent: Option<Cow<'a, str>> as Option<SizedString>,
        pub display_info: Option<AdvancementDisplayInfo<'a>>,
        pub requirements: AdvancementRequirements<'a>,
        pub sends_telemetry: bool as Single
    }
}

slice_serializable! {
    #[derive(Debug, Clone)]
    pub struct NamedAdvancement<'a> {
        pub id: Cow<'a, str> as SizedString,
        pub advancement: Advancement<'a>
    }
}

slice_serializable! {
    #[derive(Debug)]
    pub struct CriterionProgress<'a> {
        pub id: Cow<'a, str> as SizedString,
        pub time: Option<u64> as Option<BigEndian>
    }
}

slice_serializable! {
    #[derive(Debug)]
    pub struct NamedAdvancementProgress<'a> {
        pub id: Cow<'a, str> as SizedString,
        pub criteria: Vec<CriterionProgress<'a>> as SizedArray<CriterionProgress<'a>>
    }
}

slice_serializable! {
    #[derive(Debug)]
    pub struct UpdateAdvancements<'a> {
        pub reset: bool as Single,
        pub added: Cow<'a, [NamedAdvancement<'a>]> as SizedArray<NamedAdvancement<'a>>,
        pub removed: Vec<Cow<'a, str>> as SizedArray<SizedString>,
        pub progress: Vec<NamedAdvancementProgress<'a>> as SizedArray<NamedAdvancementProgress<'a>>,
        pub show_advancements: bool as Single
    }
}

// Update Attributes
slice_serializable! {
    #[derive(Debug)]
    pub struct UpdateAttributes<'a> {
        pub entity_id: i32 as VarInt,
        pub attribute: Cow<'a, [AttributeEntry<'a>]> as SizedArray<AttributeEntry>,
    }
}

slice_serializable! {
    #[derive(Clone, Debug)]
    pub struct AttributeEntry<'a> {
        pub id: Attribute as AttemptFrom<VarInt, u8>,
        pub value: f64 as BigEndian,
        pub modifiers: Vec<AttributeModifier<'a>> as SizedArray<AttributeModifier>
    }
}

slice_serializable! {
    #[derive(Clone, Debug)]
    pub struct AttributeModifier<'a> {
        pub id: Cow<'a, str> as SizedString<256>,
        pub amount: f64 as BigEndian,
        pub operation: u8 as Single,
        
    }
}

// Update Tags
slice_serializable! {
    #[derive(Debug)]
    pub struct Tag<'a> {
        pub name: &'a str as SizedString,
        pub entries: Vec<u16> as SizedArray<VarInt>
    }
}

slice_serializable! {
    #[derive(Debug)]
    pub struct TagRegistry<'a> {
        // Tag identifier (Vanilla required tags are minecraft:block, minecraft:item, minecraft:fluid, minecraft:entity_type, and minecraft:game_event)
        pub tag_type: &'a str as SizedString,
        pub values: Vec<Tag<'a>> as SizedArray<Tag>
    }
}

slice_serializable! {
    #[derive(Debug)]
    pub struct UpdateTags<'a> {
        pub registries: Vec<TagRegistry<'a>> as SizedArray<TagRegistry>
    }
}
