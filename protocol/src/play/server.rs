use binary::slice_serialization::*;

use crate::identify_packets;
use crate::types::{
    BlockPosition, ByteRotation, CommandNode, GameProfile, QuantizedShort, SignatureData, ProtocolItemStack,
};
use crate::IdentifiedPacket;
use num_enum::{TryFromPrimitive, IntoPrimitive};

identify_packets! {
    PacketId,
    AddEntity = 0x00,
    // AddExperienceOrb = 0x01,
    AddPlayer = 0x02,
    AnimateEntity = 0x03,
    // AwardStats = 0x04,
    // BlockChangedAck = 0x05,
    // BlockDestruction = 0x06,
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
    ContainerSetSlot = 0x13,
    // Cooldown = 0x14,
    // CustomChatCompletions = 0x15,
    CustomPayload<'_> = 0x16,
    // CustomSound = 0x17,
    // DeleteChat = 0x18,
    // Disconnect = 0x19,
    // EntityEvent = 0x1a,
    // Explode = 0x1b,
    // ForgetLevelChunk = 0x1c,
    // GameEvent = 0x1d,
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
    // MoveEntityPos = 0x28,
    // MoveEntityPosRot = 0x29,
    // MoveEntityRot = 0x2a,
    // MoveVehicle = 0x2b,
    // OpenBook = 0x2c,
    // OpenScreen = 0x2d,
    // OpenSignEditor = 0x2e,
    // Ping = 0x2f,
    // PlaceGhostRecipe = 0x30,
    // PlayerAbilities = 0x31,
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
    // SetEntityData = 0x50,
    // SetEntityLink = 0x51,
    // SetEntityMotion = 0x52,
    // SetEquipment = 0x53,
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
    TeleportEntity = 0x66
    // UpdateAdvancements = 0x65,
    // UpdateAttributes = 0x66,
    // UpdateMobEffect = 0x67,
    // UpdateRecipes = 0x68,
    // UpdateTags = 0x69,
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
    MagicCriticalHit
}

// Animate Entity
slice_serializable! {
    #[derive(Debug)]
    pub struct AnimateEntity {
        pub id: i32 as VarInt,
        pub animation: EntityAnimation as AttemptFrom<Single, u8>
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
    pub struct ContainerSetSlot {
        pub window_id: i8 as Single,
        pub state_id: i32 as VarInt,
        pub slot: i16 as BigEndian,
        pub item: Option<ProtocolItemStack>
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

#[derive(Debug, Copy, Clone, TryFromPrimitive, IntoPrimitive)]
#[repr(u16)]
pub enum LevelEventType {
    SOUND_DISPENSER_DISPENSE = 1000,
    SOUND_DISPENSER_FAIL = 1001,
    SOUND_DISPENSER_PROJECTILE_LAUNCH = 1002,
    SOUND_ENDER_EYE_LAUNCH = 1003,
    SOUND_FIREWORK_SHOOT = 1004,
    SOUND_OPEN_IRON_DOOR = 1005,
    SOUND_OPEN_WOODEN_DOOR = 1006,
    SOUND_OPEN_WOODEN_TRAP_DOOR = 1007,
    SOUND_OPEN_FENCE_GATE = 1008,
    SOUND_EXTINGUISH_FIRE = 1009,
    SOUND_PLAY_RECORDING = 1010,
    SOUND_CLOSE_IRON_DOOR = 1011,
    SOUND_CLOSE_WOODEN_DOOR = 1012,
    SOUND_CLOSE_WOODEN_TRAP_DOOR = 1013,
    SOUND_CLOSE_FENCE_GATE = 1014,
    SOUND_GHAST_WARNING = 1015,
    SOUND_GHAST_FIREBALL = 1016,
    SOUND_DRAGON_FIREBALL = 1017,
    SOUND_BLAZE_FIREBALL = 1018,
    SOUND_ZOMBIE_WOODEN_DOOR = 1019,
    SOUND_ZOMBIE_IRON_DOOR = 1020,
    SOUND_ZOMBIE_DOOR_CRASH = 1021,
    SOUND_WITHER_BLOCK_BREAK = 1022,
    SOUND_WITHER_BOSS_SPAWN = 1023,
    SOUND_WITHER_BOSS_SHOOT = 1024,
    SOUND_BAT_LIFTOFF = 1025,
    SOUND_ZOMBIE_INFECTED = 1026,
    SOUND_ZOMBIE_CONVERTED = 1027,
    SOUND_DRAGON_DEATH = 1028,
    SOUND_ANVIL_BROKEN = 1029,
    SOUND_ANVIL_USED = 1030,
    SOUND_ANVIL_LAND = 1031,
    SOUND_PORTAL_TRAVEL = 1032,
    SOUND_CHORUS_GROW = 1033,
    SOUND_CHORUS_DEATH = 1034,
    SOUND_BREWING_STAND_BREW = 1035,
    SOUND_CLOSE_IRON_TRAP_DOOR = 1036,
    SOUND_OPEN_IRON_TRAP_DOOR = 1037,
    SOUND_END_PORTAL_SPAWN = 1038,
    SOUND_PHANTOM_BITE = 1039,
    SOUND_ZOMBIE_TO_DROWNED = 1040,
    SOUND_HUSK_TO_ZOMBIE = 1041,
    SOUND_GRINDSTONE_USED = 1042,
    SOUND_PAGE_TURN = 1043,
    SOUND_SMITHING_TABLE_USED = 1044,
    SOUND_POINTED_DRIPSTONE_LAND = 1045,
    SOUND_DRIP_LAVA_INTO_CAULDRON = 1046,
    SOUND_DRIP_WATER_INTO_CAULDRON = 1047,
    SOUND_SKELETON_TO_STRAY = 1048,
    COMPOSTER_FILL = 1500,
    LAVA_FIZZ = 1501,
    REDSTONE_TORCH_BURNOUT = 1502,
    END_PORTAL_FRAME_FILL = 1503,
    DRIPSTONE_DRIP = 1504,
    PARTICLES_AND_SOUND_PLANT_GROWTH = 1505,
    PARTICLES_SHOOT = 2000,
    PARTICLES_DESTROY_BLOCK = 2001,
    PARTICLES_SPELL_POTION_SPLASH = 2002,
    PARTICLES_EYE_OF_ENDER_DEATH = 2003,
    PARTICLES_MOBBLOCK_SPAWN = 2004,
    PARTICLES_PLANT_GROWTH = 2005,
    PARTICLES_DRAGON_FIREBALL_SPLASH = 2006,
    PARTICLES_INSTANT_POTION_SPLASH = 2007,
    PARTICLES_DRAGON_BLOCK_BREAK = 2008,
    PARTICLES_WATER_EVAPORATING = 2009,
    ANIMATION_END_GATEWAY_SPAWN = 3000,
    ANIMATION_DRAGON_SUMMON_ROAR = 3001,
    PARTICLES_ELECTRIC_SPARK = 3002,
    PARTICLES_AND_SOUND_WAX_ON = 3003,
    PARTICLES_WAX_OFF = 3004,
    PARTICLES_SCRAPE = 3005,
    PARTICLES_SCULK_CHARGE = 3006,
    PARTICLES_SCULK_SHRIEK = 3007,
}

// Level Event
slice_serializable! {
    #[derive(Debug)]
    pub struct LevelEvent {
        pub id: u64 as BigEndian
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
