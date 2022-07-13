use binary::slice_serialization::{self, SliceSerializable};
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
pub enum Action {
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
