use binary::slice_serialization::{
    self, BigEndian, Single, SizedArray, SizedString, SliceSerializable, VarInt,
};
use num_enum::{IntoPrimitive, TryFromPrimitive};

#[derive(Default, Debug, Copy, Clone, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum ChatVisibility {
    #[default]
    Full,
    System,
    None,
}

#[derive(Default, Debug, Copy, Clone, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum ArmPosition {
    #[default]
    Right,
    Left,
}

#[derive(Debug, Copy, Clone, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum HandAction {
    StartDestroyBlock,
    AbortDestroyBlock,
    StopDestroyBlock,
    DropAllItems,
    DropItem,
    ReleaseUseItem,
    SwapItemWithOffHand,
}

#[derive(Debug, Copy, Clone, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum MoveAction {
    PressShiftKey,
    ReleaseShiftKey,
    StopSleeping,
    StartSprinting,
    StopSprinting,
    StartRidingJump,
    StopRidingJump,
    OpenInventory,
    StartFallFlying,
}

#[derive(Debug, Copy, Clone, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum Direction {
    Down,
    Up,
    North,
    South,
    West,
    East,
}

// Block Position

#[derive(Debug, Copy, Clone)]
pub struct BlockPosition {
    x: i32,
    y: i16,
    z: i32,
}

impl SliceSerializable<'_> for BlockPosition {
    type RefType = BlockPosition;

    fn maybe_deref(t: &Self) -> Self::RefType {
        *t
    }

    fn read(bytes: &mut &[u8]) -> anyhow::Result<Self> {
        let value: i64 = slice_serialization::BigEndian::read(bytes)?;

        Ok(Self {
            x: (value >> 38) as i32,
            y: (value << 52 >> 52) as i16,
            z: (value << 26 >> 38) as i32,
        })
    }

    unsafe fn write(bytes: &mut [u8], data: Self) -> &mut [u8] {
        let value = ((data.x as i64 & 0x3FFFFFF) << 38)
            | ((data.z as i64 & 0x3FFFFFF) << 12)
            | (data.y as i64 & 0xFFF);

        <slice_serialization::BigEndian as SliceSerializable<'_, i64>>::write(bytes, value)
    }

    fn get_write_size(_: Self) -> usize {
        <slice_serialization::BigEndian as SliceSerializable<'_, i64>>::get_write_size(0)
    }
}

// Command Node

#[derive(Debug, Clone)]
pub enum CommandNode {
    Root {
        children: Vec<i32>,
    },
    Literal {
        children: Vec<i32>,
        is_executable: bool,
        redirect: Option<i32>,
        name: &'static str,
    },
    Argument {
        children: Vec<i32>,
        is_executable: bool,
        redirect: Option<i32>,
        suggestion: Option<SuggestionType>,
        name: &'static str,
        parser: CommandNodeParser,
    },
}

impl<'a> SliceSerializable<'a> for CommandNode {
    type RefType = &'a Self;

    fn maybe_deref(t: &'a Self) -> Self::RefType {
        t
    }

    fn read(_: &mut &'a [u8]) -> anyhow::Result<Self> {
        unimplemented!();
    }

    unsafe fn write(mut bytes: &mut [u8], data: Self::RefType) -> &mut [u8] {
        match data {
            CommandNode::Root { children } => {
                let flags = 0; // root type
                let bytes = <Single as SliceSerializable<'_, u8>>::write(bytes, flags);
                SizedArray::<VarInt>::write(bytes, children)
            }
            CommandNode::Literal {
                children,
                is_executable: executable,
                redirect,
                name,
            } => {
                let mut flags = 1; // literal type
                flags |= if *executable { 4 } else { 0 };
                flags |= if redirect.is_some() { 8 } else { 0 };

                bytes = <Single as SliceSerializable<'_, u8>>::write(bytes, flags);
                bytes = SizedArray::<VarInt>::write(bytes, children);

                if let Some(redirect) = redirect {
                    VarInt::write(bytes, *redirect);
                }

                SizedString::<0>::write(bytes, name)
            }
            CommandNode::Argument {
                children,
                is_executable: executable,
                redirect,
                suggestion,
                name,
                parser,
            } => {
                let mut flags = 2; // argument type
                flags |= if *executable { 4 } else { 0 };
                flags |= if redirect.is_some() { 8 } else { 0 };
                flags |= if suggestion.is_some() { 16 } else { 0 };

                bytes = <Single as SliceSerializable<'_, u8>>::write(bytes, flags);
                bytes = SizedArray::<VarInt>::write(bytes, children);

                if let Some(redirect) = redirect {
                    VarInt::write(bytes, *redirect);
                }

                bytes = SizedString::<0>::write(bytes, name);

                bytes = CommandNodeParser::write(bytes, *parser);

                if let Some(suggestion) = suggestion {
                    SizedString::<0>::write(bytes, (*suggestion).into());
                }

                bytes
            }
        }
    }

    fn get_write_size(data: &'a Self) -> usize {
        const VARINT_MAX: usize = 5;

        match data {
            CommandNode::Root { children } => {
                1 + // flags
                VarInt::get_write_size(children.len() as _) + // children size
                VARINT_MAX * children.len() // children
            }
            CommandNode::Literal {
                children,
                is_executable: _,
                redirect,
                name,
            } => {
                1 + // flags
                VarInt::get_write_size(children.len() as _) + // children size
                VARINT_MAX * children.len() + // children
                redirect.map_or(0, VarInt::get_write_size) + // redirect
                SizedString::<0>::get_write_size(name) // name
            }
            CommandNode::Argument {
                children,
                is_executable: _,
                redirect,
                suggestion,
                name,
                parser,
            } => {
                1 + // flags
                VarInt::get_write_size(children.len() as _) + // children size
                VARINT_MAX * children.len() + // children
                redirect.map_or(0, VarInt::get_write_size) + // redirect
                (if suggestion.is_some() { 33 } else { 0 }) +
                SizedString::<0>::get_write_size(name) + // name
                CommandNodeParser::get_write_size(*parser) // parser
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum SuggestionType {
    AskServer,
    AllRecipes,
    AvailableSounds,
    AvailableBiomes,
    SummonableEntities,
}

impl From<SuggestionType> for &'static str {
    fn from(suggestion: SuggestionType) -> Self {
        match suggestion {
            SuggestionType::AskServer => "minecraft:ask_server",
            SuggestionType::AllRecipes => "minecraft:all_recipes",
            SuggestionType::AvailableSounds => "minecraft:available_sounds",
            SuggestionType::AvailableBiomes => "minecraft:available_biomes",
            SuggestionType::SummonableEntities => "minecraft:summonable_entities",
        }
    }
}

#[derive(Debug, Copy, Clone, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum StringParserMode {
    // Reads a single word
    SingleWord,
    // If it starts with a `"`, keeps reading until another `"` (allowing escaping with \). Otherwise behaves the same as `SingleWord`
    QuotablePhrase,
    // Reads the rest of the content after the cursor. Quotes will not be removed
    GreedyPhrase,
}

#[derive(Debug, Copy, Clone)]
#[repr(C, u8)]
pub enum CommandNodeParser {
    // Boolean value (true or false, case-sensitive)
    Bool,
    // f32
    Float { min: Option<f32>, max: Option<f32> },
    // f64
    Double { min: Option<f64>, max: Option<f64> },
    // i32
    Integer { min: Option<i32>, max: Option<i32> },
    // i64
    Long { min: Option<i64>, max: Option<i64> },
    // String
    String { mode: StringParserMode },
    // Entity or online player
    // Selector (@p, @e, etc.), player name ("Moulberry") or uuid ("d0e05de7-6067-454d-beae-c6d19d886191")
    Entity { single: bool, player_only: bool },
    // Player, online or offline
    // Selector (@p, etc.), player name ("Moulberry") or uuid ("d0e05de7-6067-454d-beae-c6d19d886191")
    GameProfile,
    // A block location, represented as 3 integers. May use relative locations with ~
    // eg. "10 ~5 -3"
    BlockPos,
    // A column (chunk section) location, represented as 3 integers. May use relative locations with ~
    // eg. "10 ~5 -3"
    ColumnPos,
    // A location, represented as 3 floats. May use relative locations with ~
    // eg. "1.2 ~3.9 -7.4"
    Vec3,
    // A location, represented as 2 floats. May use relative locations with ~
    // eg. "1.2 ~3.9"
    Vec2,
    // A block state, optionally including NBT and state information
    // eg. "minecraft:stone" or "minecraft:water[level=4]"
    BlockState,
    // A block predicate that potentially matches many block states
    // eg. "#minecraft:logs" for all logs
    BlockPredicate,
    // An item, optionally including NBT
    // eg. "minecraft:diamond_sword"
    ItemStack,
    // An item predicate that potentially matches many item stacks
    // eg. "#minecraft:fishes"
    ItemPredicate,
    // A chat color, one of https://wiki.vg/Chat#Colors or "reset"
    Color,
    // A json chat component
    // eg. `{"text": "Hello!", "color": "red"}`
    Component,
    // ???
    Message,
    // An NBT value as JSON
    NBT,
    // Partial (incomplete) NBT tag
    NBTTag,
    // A path within an NBT value, allowing for array and member accesses
    NBTPath,
    // A scoreboard objective
    Objective,
    // A single score criterion
    ObjectiveCriteria,
    // A scoreboard operator
    Operation,
    // A particle effect (an identifier with optional extra information, see https://wiki.vg/Protocol#Particle packet)
    Particle,
    // ??? (maybe like rotation but only 1)
    Angle,
    // Yaw and pitch, represented as 2 floats. May use relative rotations with ~
    // eg. "90 ~0"
    Rotation,
    // A scoreboard display position slot.
    // eg. `list`, or `sidebar`, or `belowName`, or `sidebar.team.${color}` for all of https://wiki.vg/Chat#Colors
    ScoreboardSlot,
    // Something that can join a team. Allows selectors and *
    ScoreHolder { allow_many: bool },
    // A collection of up to 3 axes
    // eg. "x y" or "z"
    Swizzle,
    // The name of a team. Parsed as an unquoted string
    Team,
    // A name for an inventory slot
    // eg. "weapon.mainhand". See https://minecraft.fandom.com/wiki/Slot#Command_argument
    ItemSlot,
    // An Identifier
    // eg. "minecraft:textures/wool.png"
    ResourceLocation,
    // A potion effect
    MobEffect,
    // A function (???)
    Function,
    // The entity anchor related to the facing argument in the teleport command
    // eg. "feet" or "eyes"
    EntityAnchor,
    // An integer range with a min and max
    // eg. 0..5 or 10..
    IntRange,
    // A float range with a min and max
    // eg. 1.2..7 or 36.7..
    FloatRange,
    // An item enchantment
    // eg. "minecraft:sharpness"
    ItemEnchantment,
    // Entity summon
    // eg. "minecraft:zombie"
    EntitySummon,
    // Represents a dimension
    // eg. "minecraft:overworld" or "minecraft:the_end"
    Dimension,
    // Represents a time duration
    // eg. "5s" (seconds) or "7d" (days) or "24000t" (ticks)
    Time,
    // An identifier ("minecraft:sand") or a tag name ("#minecraft:beds") for a registry
    ResourceOrTag { registry: &'static str },
    // An identifier ("minecraft:sand") for a registry
    Resource { registry: &'static str },
    // ???
    TemplateMirror,
    TemplateRotation,
    // A uuid value
    // eg. "d0e05de7-6067-454d-beae-c6d19d886191"
    UUID,
}

impl From<CommandNodeParser> for u8 {
    fn from(parser: CommandNodeParser) -> Self {
        unsafe { std::mem::transmute(std::mem::discriminant(&parser)) }
    }
}

impl SliceSerializable<'_> for CommandNodeParser {
    type RefType = Self;

    fn maybe_deref(t: &Self) -> Self::RefType {
        *t
    }

    fn read(_: &mut &[u8]) -> anyhow::Result<Self> {
        unimplemented!()
    }

    unsafe fn write(mut bytes: &mut [u8], data: Self) -> &mut [u8] {
        let self_id: u8 = data.into();

        bytes = <Single as SliceSerializable<'_, u8>>::write(bytes, self_id);
        match data {
            CommandNodeParser::Float { min, max } => {
                write_optional_min_max::<BigEndian, _>(bytes, min, max)
            }
            CommandNodeParser::Double { min, max } => {
                write_optional_min_max::<BigEndian, _>(bytes, min, max)
            }
            CommandNodeParser::Integer { min, max } => {
                write_optional_min_max::<BigEndian, _>(bytes, min, max)
            }
            CommandNodeParser::Long { min, max } => {
                write_optional_min_max::<BigEndian, _>(bytes, min, max)
            }
            CommandNodeParser::String { mode } => {
                <Single as SliceSerializable<'_, u8>>::write(bytes, mode as u8)
            }
            CommandNodeParser::Entity {
                single,
                player_only,
            } => {
                let flags = if single { 1 } else { 0 } | if player_only { 2 } else { 0 };
                <Single as SliceSerializable<'_, u8>>::write(bytes, flags)
            }
            CommandNodeParser::ScoreHolder { allow_many } => {
                <Single as SliceSerializable<'_, bool>>::write(bytes, allow_many)
            }
            CommandNodeParser::ResourceOrTag { registry } => {
                SizedString::<0>::write(bytes, registry)
            }
            CommandNodeParser::Resource { registry } => SizedString::<0>::write(bytes, registry),
            _ => bytes,
        }
    }

    fn get_write_size(data: Self) -> usize {
        1 + // id of self
        match data {
            CommandNodeParser::Float { min, max } => {
                1 + if min.is_some() { 4 } else { 0 } + if max.is_some() { 4 } else { 0 }
            },
            CommandNodeParser::Double { min, max } => {
                1 + if min.is_some() { 8 } else { 0 } + if max.is_some() { 8 } else { 0 }
            },
            CommandNodeParser::Integer { min, max } => {
                1 + if min.is_some() { 4 } else { 0 } + if max.is_some() { 4 } else { 0 }
            },
            CommandNodeParser::Long { min, max } => {
                1 + if min.is_some() { 8 } else { 0 } + if max.is_some() { 8 } else { 0 }
            },
            CommandNodeParser::String { mode: _ } => 1,
            CommandNodeParser::Entity { single: _, player_only: _ } => 1,
            CommandNodeParser::ScoreHolder { allow_many: _ } => 1,
            CommandNodeParser::ResourceOrTag { registry } => 1 + registry.len(),
            CommandNodeParser::Resource { registry } => 1 + registry.len(),
            _ => 0,
        }
    }
}

unsafe fn write_optional_min_max<'a, S, T>(
    mut bytes: &mut [u8],
    min: Option<T>,
    max: Option<T>,
) -> &mut [u8]
where
    S: SliceSerializable<'a, T, RefType = T>,
{
    let flags: u8 = if min.is_some() { 1 } else { 0 } | if max.is_some() { 2 } else { 0 };
    bytes = <Single as SliceSerializable<'_, u8>>::write(bytes, flags);

    if let Some(min) = min {
        bytes = S::write(bytes, min);
    }
    if let Some(max) = max {
        bytes = S::write(bytes, max);
    }
    bytes
}
