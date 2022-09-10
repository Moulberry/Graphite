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

    fn get_write_size(_: i32) -> usize {
        5 // todo: we could calculate the needed bits as a time->space tradeoff. probably not worth?
    }

    unsafe fn write(bytes: &mut [u8], data: i32) -> &mut [u8] {
        debug_assert!(
            bytes.len() >= 5,
            "invariant: slice must contain at least 5 bytes to perform varint_i32 write"
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

            fn get_write_size(_: $typ) -> usize {
                5 // todo: we could calculate the needed bits as a time->space tradeoff. probably not worth?
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