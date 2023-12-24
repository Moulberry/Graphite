use std::io::Cursor;

use super::*;
pub enum VarInt {}

impl SliceSerializable<'_, i32> for VarInt {
    type CopyType = i32;

    fn read(bytes: &mut &[u8]) -> anyhow::Result<i32> {
        if bytes.is_empty() {
            return Err(BinaryReadError::NotEnoughRemainingBytes.into());
        }

        let (num, size) = crate::varint::decode::i32(bytes)?;
        *bytes = &bytes[size..];
        Ok(num)
    }

    fn get_write_size(num: i32) -> usize {
        crate::varint::encode::needed_bytes(num)
    }

    unsafe fn write(bytes: &mut [u8], data: i32) -> &mut [u8] {
        debug_assert!(
            bytes.len() >= crate::varint::encode::needed_bytes(data),
            "invariant: slice must contain enough bytes to perform varint_i32 write"
        );

        let (encoded, size) = crate::varint::encode::i32_raw(data);
        bytes[..size].clone_from_slice(&encoded[..size]);
        &mut bytes[size..]
    }

    #[inline(always)]
    fn as_copy_type(t: &i32) -> Self::CopyType {
        *t
    }
}

macro_rules! for_primitive {
    ($typ:tt) => {
        impl SliceSerializable<'_, $typ> for VarInt {
            type CopyType = $typ;

            fn read(bytes: &mut &[u8]) -> anyhow::Result<$typ> {
                Ok(<VarInt as SliceSerializable<i32>>::read(bytes)? as $typ)
            }

            fn get_write_size(num: $typ) -> usize {
                <VarInt as SliceSerializable<i32>>::get_write_size(num as i32)
            }

            unsafe fn write<'b>(bytes: &'b mut [u8], primitive: $typ) -> &'b mut [u8] {
                <VarInt as SliceSerializable<i32>>::write(bytes, primitive as i32)
            }

            #[inline(always)]
            fn as_copy_type(t: &$typ) -> Self::CopyType {
                *t
            }
        }
    };
}

for_primitive!(u16);
for_primitive!(u32);
for_primitive!(usize);

impl SliceSerializable<'_, i64> for VarInt {
    type CopyType = i64;

    fn read(bytes: &mut &[u8]) -> anyhow::Result<i64> {
        if bytes.is_empty() {
            return Err(BinaryReadError::NotEnoughRemainingBytes.into());
        }

        let mut cursor = Cursor::new(*bytes);
        let num = leb128::read::signed(&mut cursor)?;
        *bytes = &bytes[cursor.position() as usize..];
        Ok(num)
    }

    fn get_write_size(_: i64) -> usize {
        10
    }

    unsafe fn write(bytes: &mut [u8], data: i64) -> &mut [u8] {
        debug_assert!(
            bytes.len() >= 10,
            "invariant: slice must contain enough bytes to perform varint_i64 write"
        );

        let mut cursor = Cursor::new(bytes);
        let written = leb128::write::signed(&mut cursor, data).unwrap();
        &mut (cursor.into_inner())[written..]
    }

    #[inline(always)]
    fn as_copy_type(t: &i64) -> Self::CopyType {
        *t
    }
}