use graphite_binary::nbt::CachedNBT;
use graphite_binary::slice_serialization::*;
use std::borrow::Cow;

use crate::identify_packets;
use crate::types::{
    BlockPosition, ByteRotation, CommandNode, EquipmentList, EquipmentSlot, GameProfile,
    ProtocolItemStack, QuantizedShort, SignatureData,
};
use crate::IdentifiedPacket;
use num_enum::{IntoPrimitive, TryFromPrimitive};

identify_packets! {
    PacketId,
    AddEntity = 0x00,
    // AddExperienceOrb = 0x01,
    AddPlayer = 0x02,
    AnimateEntity = 0x03,
    // AwardStats = 0x04,
    BlockChangedAck = 0x05,
    BlockDestruction = 0x06,
    // BlockEntityData = 0x07,
    // BlockEvent = 0x08,
    BlockUpdate = 0x09,
    // BossEvent = 0x0a,
    // ChangeDifficulty = 0x0b,
    // ChatPreview = 0x0c,
    // ClearTitles = 0x0d,
    // CommandSuggestions = 0x0e,
    Commands = 0x0f,
    // ContainerClose = 0x10,
    // ContainerSetContent = 0x11,
    // ContainerSetData = 0x12,
    ContainerSetSlot<'_> = 0x13,
    // Cooldown = 0x14,
    // CustomChatCompletions = 0x15,
    CustomPayload<'_> = 0x16,
    // CustomSound = 0x17,
    // DeleteChat = 0x18,
    // Disconnect = 0x19,
    // EntityEvent = 0x1a,
    // Explode = 0x1b,
    // ForgetLevelChunk = 0x1c,
    GameEvent = 0x1d,
    // HorseScreenOpen = 0x1e,
    // InitializeBorder = 0x1f,
    KeepAlive = 0x20,
    LevelChunkWithLight<'_> = 0x21,
    LevelEvent = 0x22,
    // LevelParticles = 0x23,
    // LightUpdate = 0x24,
    Login<'_> = 0x25,
    // MapItemData = 0x26,
    // MerchantOffers = 0x27,
    MoveEntityPos = 0x28,
    MoveEntityPosRot = 0x29,
    MoveEntityRot = 0x2a,
    // MoveVehicle = 0x2b,
    // OpenBook = 0x2c,
    // OpenScreen = 0x2d,
    // OpenSignEditor = 0x2e,
    // Ping = 0x2f,
    // PlaceGhostRecipe = 0x30,
    PlayerAbilities = 0x31,
    // PlayerChatHeader = 0x32,
    // PlayerChat = 0x33,
    // PlayerCombatEnd = 0x34,
    // PlayerCombatEnter = 0x35,
    // PlayerCombatKill = 0x36,
    PlayerInfo<'_> = 0x37,
    // PlayerLookAt = 0x38,
    PlayerPosition = 0x39,
    // UnlockRecipe = 0x3a,
    RemoveEntities = 0x3b,
    // RemoveMobEffect = 0x3c,
    // ResourcePack = 0x3d,
    // Respawn = 0x3e,
    RotateHead = 0x3f,
    // SectionBlocksUpdate = 0x40,
    // SelectAdvancementTab = 0x41,
    // ServerData = 0x42,
    // SetActionBarText = 0x43,
    // SetBorderCenter = 0x44,
    // SetBorderLerpSize = 0x45,
    // SetBorderSize = 0x46,
    // SetBorderWarningDelay = 0x47,
    // SetBorderWarningDistance = 0x48,
    // SetCamera = 0x49,
    // SetCarriedItem = 0x4a,
    SetChunkCacheCenter = 0x4b,
    // SetChunkCacheRadius = 0x4c,
    // SetDefaultSpawnPosition = 0x4d,
    // SetDisplayChatPreview = 0x4e,
    // SetDisplayObjective = 0x4f,
    SetEntityData<'_> = 0x50,
    // SetEntityLink = 0x51,
    // SetEntityMotion = 0x52,
    SetEquipment<'_> = 0x53,
    // SetExperience = 0x54,
    // SetHealth = 0x55,
    // SetObjective = 0x56,
    // SetPassengers = 0x57,
    // SetPlayerTeam = 0x58,
    // SetScore = 0x59,
    // SetSimulationDistance = 0x5a,
    // SetSubtitleText = 0x5b,
    // SetTime = 0x5c,
    // SetTitleText = 0x5d,
    // SetTitleAnimation = 0x5e,
    // SoundEntity = 0x5f,
    // Sound = 0x60,
    // StopSound = 0x61,
    SystemChat<'_> = 0x62,
    // TabList = 0x63,
    // TagQuery = 0x64,
    // TakeItemEntity = 0x65,
    TeleportEntity = 0x66,
    // UpdateAdvancements = 0x67,
    // UpdateAttributes = 0x68,
    // UpdateMobEffect = 0x69,
    // UpdateRecipes = 0x6f,
    UpdateTags<'_> = 0x6b
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

// Animate Entity
#[derive(Default, Debug, Copy, Clone, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum EntityAnimation {
    #[default]
    SwingMainHand,
    Hurt,
    WakeUp,
    SwingOffHand,
    CriticalHit,
    MagicCriticalHit,
}

// Animate Entity
slice_serializable! {
    #[derive(Debug)]
    pub struct AnimateEntity {
        pub id: i32 as VarInt,
        pub animation: EntityAnimation as AttemptFrom<Single, u8>
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

// Commands
slice_serializable! {
    #[derive(Debug)]
    pub struct Commands {
        pub nodes: Vec<CommandNode> as SizedArray<CommandNode>,
        pub root_index: i32 as VarInt
    }
}

// Container Set Slot
slice_serializable! {
    #[derive(Debug)]
    pub struct ContainerSetSlot<'a> {
        pub window_id: i8 as Single,
        pub state_id: i32 as VarInt,
        pub slot: i16 as BigEndian,
        pub item: Option<ProtocolItemStack<'a>>
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
}

slice_serializable! {
    #[derive(Debug)]
    pub struct GameEvent {
        pub event_type: GameEventType as AttemptFrom<Single, u8>,
        pub param: f32 as BigEndian
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
        pub heightmaps: Cow<'a, CachedNBT> as NBTBlob,
        pub data: &'a [u8] as SizedBlob,
        pub block_entity_count: i32 as VarInt,
        pub block_entity_data: &'a [u8] as WriteOnlyBlob,
        pub trust_edges: bool as Single
    }
}

#[derive(Debug, Copy, Clone, TryFromPrimitive, IntoPrimitive)]
#[repr(i32)]
pub enum LevelEventType {
    SoundDispenserDispense = 1000,
    SoundDispenserFail = 1001,
    SoundDispenserProjectileLaunch = 1002,
    SoundEnderEyeLaunch = 1003,
    SoundFireworkShoot = 1004,
    SoundOpenIronDoor = 1005,
    SoundOpenWoodenDoor = 1006,
    SoundOpenWoodenTrapDoor = 1007,
    SoundOpenFenceGate = 1008,
    SoundExtinguishFire = 1009,
    SoundPlayRecording = 1010,
    SoundCloseIronDoor = 1011,
    SoundCloseWoodenDoor = 1012,
    SoundCloseWoodenTrapDoor = 1013,
    SoundCloseFenceGate = 1014,
    SoundGhastWarning = 1015,
    SoundGhastFireball = 1016,
    SoundDragonFireball = 1017,
    SoundBlazeFireball = 1018,
    SoundZombieWoodenDoor = 1019,
    SoundZombieIronDoor = 1020,
    SoundZombieDoorCrash = 1021,
    SoundWitherBlockBreak = 1022,
    SoundWitherBossSpawn = 1023,
    SoundWitherBossShoot = 1024,
    SoundBatLiftoff = 1025,
    SoundZombieInfected = 1026,
    SoundZombieConverted = 1027,
    SoundDragonDeath = 1028,
    SoundAnvilBroken = 1029,
    SoundAnvilUsed = 1030,
    SoundAnvilLand = 1031,
    SoundPortalTravel = 1032,
    SoundChorusGrow = 1033,
    SoundChorusDeath = 1034,
    SoundBrewingStandBrew = 1035,
    SoundCloseIronTrapDoor = 1036,
    SoundOpenIronTrapDoor = 1037,
    SoundEndPortalSpawn = 1038,
    SoundPhantomBite = 1039,
    SoundZombieToDrowned = 1040,
    SoundHuskToZombie = 1041,
    SoundGrindstoneUsed = 1042,
    SoundPageTurn = 1043,
    SoundSmithingTableUsed = 1044,
    SoundPointedDripstoneLand = 1045,
    SoundDripLavaIntoCauldron = 1046,
    SoundDripWaterIntoCauldron = 1047,
    SoundSkeletonToStray = 1048,
    ComposterFill = 1500,
    LavaFizz = 1501,
    RedstoneTorchBurnout = 1502,
    EndPortalFrameFill = 1503,
    DripstoneDrip = 1504,
    ParticlesAndSoundPlantGrowth = 1505,
    ParticlesShoot = 2000,
    ParticlesDestroyBlock = 2001,
    ParticlesSpellPotionSplash = 2002,
    ParticlesEyeOfEnderDeath = 2003,
    ParticlesMobblockSpawn = 2004,
    ParticlesPlantGrowth = 2005,
    ParticlesDragonFireballSplash = 2006,
    ParticlesInstantPotionSplash = 2007,
    ParticlesDragonBlockBreak = 2008,
    ParticlesWaterEvaporating = 2009,
    AnimationEndGatewaySpawn = 3000,
    AnimationDragonSummonRoar = 3001,
    ParticlesElectricSpark = 3002,
    ParticlesAndSoundWaxOn = 3003,
    ParticlesWaxOff = 3004,
    ParticlesScrape = 3005,
    ParticlesSculkCharge = 3006,
    ParticlesSculkShriek = 3007,
}

// Level Event
slice_serializable! {
    #[derive(Debug)]
    pub struct LevelEvent {
        pub event_type: LevelEventType as AttemptFrom<BigEndian, i32>,
        pub pos: BlockPosition,
        pub data: i32 as BigEndian,
        pub global: bool as Single // global used by vanilla for dragon death and portal opening
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
        pub registry_codec: Cow<'a, CachedNBT> as NBTBlob,
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
    pub struct PlayerPosition {
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

// Set Equipment
slice_serializable! {
    #[derive(Debug)]
    pub struct SetEquipment<'a> {
        pub entity_id: i32 as VarInt,
        pub equipment: Vec<(EquipmentSlot, Option<ProtocolItemStack<'a>>)> as EquipmentList
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

// Update Tags
slice_serializable! {
    #[derive(Debug)]
    pub struct Tag<'a> {
        // Tag identifier (Vanilla required tags are minecraft:block, minecraft:item, minecraft:fluid, minecraft:entity_type, and minecraft:game_event)
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
