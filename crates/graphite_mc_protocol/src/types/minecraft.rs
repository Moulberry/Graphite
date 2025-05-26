use std::borrow::Cow;

use enumset::EnumSet;
use graphite_binary::slice_serialization::*;
use graphite_mc_constants::{builtin::DataComponentType, types::{Direction, EquipmentSlot}};

use crate::types::item_stack::ItemStack;

use super::hashed_stack::HashedStack;

slice_serializable! {
    #[derive(Debug)]
    pub struct ChangedSlot {
        pub slot: i16 as BigEndian,
        pub item: Option<HashedStack>
    }
}

// Game Profile

// Note: Currently the only property that is used by the vanilla
// client is "textures", for the skin of the player
slice_serializable! {
    #[derive(Debug, Clone)]
    pub struct GameProfileProperty<'a> {
        pub id: Cow<'a, str> as SizedString,
        pub value: Cow<'a, str> as SizedString,
        pub signature: Option<Cow<'a, str>> as Option<SizedString>
    }
}

slice_serializable! {
    #[derive(Debug, Clone)]
    pub struct GameProfile<'a> {
        pub uuid: u128 as BigEndian,
        pub username: Cow<'a, str> as SizedString<16>,
        pub properties: Vec<GameProfileProperty<'a>> as SizedArray<GameProfileProperty>
    }
}

// Signature Data

slice_serializable! {
    #[derive(Debug)]
    pub struct SignatureData<'a> {
        pub timestamp: i64 as BigEndian,
        pub public_key: &'a [u8] as SizedBlob,
        pub signature: &'a [u8] as SizedBlob
    }
}

// Block Hit Result

slice_serializable! {
    #[derive(Debug)]
    pub struct BlockHitResult {
        pub position: BlockPosition,
        pub direction: Direction as AttemptFrom<Single, u8>,
        pub offset_x: f32 as BigEndian,
        pub offset_y: f32 as BigEndian,
        pub offset_z: f32 as BigEndian,
        pub is_inside: bool as Single,
        pub is_world_border_hit: bool as Single,
    }
}

// Global Position

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GlobalPosition {
    pub dimension: Cow<'static, str>,
    pub position: BlockPosition,
}

impl <'r, 'd: 'r> SliceSerializable<'r, 'd> for GlobalPosition {
    type CopyType = &'r GlobalPosition;

    fn as_copy_type(t: &'r Self) -> Self::CopyType {
        t
    }

    fn read(bytes: &mut &'d [u8]) -> anyhow::Result<Self> {
        let dimension = <SizedString as SliceSerializable<String>>::read(bytes)?;
        let position = BlockPosition::read(bytes)?;

        Ok(Self {
            dimension: Cow::Owned(dimension),
            position
        })
    }

    unsafe fn write(mut bytes: &mut [u8], data: Self::CopyType) -> &mut [u8] {
        bytes = <SizedString as SliceSerializable<&str>>::write(bytes, &data.dimension);
        bytes = BlockPosition::write(bytes, data.position);
        bytes
    }

    fn get_write_size(data: Self::CopyType) -> usize {
        <SizedString as SliceSerializable<&str>>::get_write_size(&data.dimension) +
            BlockPosition::get_write_size(data.position)
    }
}


// Equipment List (https://wiki.vg/Protocol#Set_Equipment)

pub(crate) enum EquipmentList {}

impl<'r, 'd: 'r> SliceSerializable<'r, 'd, Vec<(EquipmentSlot, ItemStack)>>
    for EquipmentList
{
    type CopyType = &'r Vec<(EquipmentSlot, ItemStack)>;

    fn as_copy_type(t: &'r Vec<(EquipmentSlot, ItemStack)>) -> Self::CopyType {
        t
    }

    fn read(
        _: &mut &'d [u8],
    ) -> anyhow::Result<Vec<(EquipmentSlot, ItemStack)>> {
        unimplemented!()
    }

    unsafe fn write(mut bytes: &mut [u8], data: Self::CopyType) -> &mut [u8] {
        let mut remaining = data.len();
        for (slot, stack) in data {
            remaining -= 1;

            let mut slot_id = *slot as u8;
            if remaining > 0 {
                slot_id |= 0b10000000;
            }

            bytes = <Single as SliceSerializable<u8>>::write(bytes, slot_id);
            bytes = ItemStack::write(bytes, stack);
        }
        bytes
    }

    fn get_write_size(data: Self::CopyType) -> usize {
        let mut size = data.len();
        for (_, stack) in data {
            size += ItemStack::get_write_size(stack)
        }
        size
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
pub struct BlockPosition {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl BlockPosition {
    pub fn new(x: i32, y: i32, z: i32) -> Self {
        Self {
            x,
            y,
            z
        }
    }

    pub fn relative(self, direction: Direction) -> Self {
        match direction {
            Direction::Down => Self {
                x: self.x,
                y: self.y - 1,
                z: self.z,
            },
            Direction::Up => Self {
                x: self.x,
                y: self.y + 1,
                z: self.z,
            },
            Direction::North => Self {
                x: self.x,
                y: self.y,
                z: self.z - 1,
            },
            Direction::South => Self {
                x: self.x,
                y: self.y,
                z: self.z + 1,
            },
            Direction::West => Self {
                x: self.x - 1,
                y: self.y,
                z: self.z,
            },
            Direction::East => Self {
                x: self.x + 1,
                y: self.y,
                z: self.z,
            },
        }
    }

    pub fn encode(&self) -> i64 {
        ((self.x as i64 & 0x3FFFFFF) << 38)
            | ((self.z as i64 & 0x3FFFFFF) << 12)
            | (self.y as i64 & 0xFFF)
    }
}

impl <'r, 'd: 'r> SliceSerializable<'r, 'd> for BlockPosition {
    type CopyType = BlockPosition;

    #[inline(always)]
    fn as_copy_type(t: &Self) -> Self::CopyType {
        *t
    }

    fn read(bytes: &mut &[u8]) -> anyhow::Result<Self> {
        let value: i64 = BigEndian::read(bytes)?;

        Ok(Self {
            x: (value >> 38) as i32,
            y: (value << 52 >> 52) as i32,
            z: (value << 26 >> 38) as i32,
        })
    }

    unsafe fn write(bytes: &mut [u8], data: Self) -> &mut [u8] {
        let value = data.encode();
        <BigEndian as SliceSerializable<i64>>::write(bytes, value)
    }

    #[inline(always)]
    fn get_write_size(_: Self) -> usize {
        <BigEndian as SliceSerializable<i64>>::get_write_size(0)
    }
}