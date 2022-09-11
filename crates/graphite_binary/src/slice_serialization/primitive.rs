use super::*;

pub enum BigEndian {}
pub enum LittleEndian {}

macro_rules! for_primitive {
    ($typ:tt, $mode:ident, $conv_from:tt, $conv_to:tt) => {
        impl SliceSerializable<'_, $typ> for $mode {
            type CopyType = $typ;

            fn read(bytes: &mut &[u8]) -> anyhow::Result<$typ> {
                const SIZE: usize = std::mem::size_of::<$typ>();

                if bytes.len() < SIZE {
                    return Err(BinaryReadError::NotEnoughRemainingBytes.into());
                }

                // Read value using conversion function
                let ret = unsafe { $typ::$conv_from(*(*bytes as *const _ as *const [_; SIZE])) };

                // Advance
                *bytes = &bytes[SIZE..];

                Ok(ret)
            }

            fn get_write_size(_: $typ) -> usize {
                std::mem::size_of::<$typ>()
            }

            unsafe fn write<'b>(bytes: &'b mut [u8], primitive: $typ) -> &'b mut [u8] {
                const SIZE: usize = std::mem::size_of::<$typ>();

                debug_assert!(
                    bytes.len() >= SIZE,
                    "invariant: slice must contain at least {} bytes to perform $func",
                    SIZE
                );

                bytes[..SIZE].clone_from_slice(&$typ::$conv_to(primitive));
                &mut bytes[SIZE..]
            }

            #[inline(always)]
            fn as_copy_type(t: &$typ) -> Self::CopyType {
                *t
            }
        }
    };
}

for_primitive!(u16, BigEndian, from_be_bytes, to_be_bytes);
for_primitive!(i16, BigEndian, from_be_bytes, to_be_bytes);
for_primitive!(u32, BigEndian, from_be_bytes, to_be_bytes);
for_primitive!(i32, BigEndian, from_be_bytes, to_be_bytes);
for_primitive!(u64, BigEndian, from_be_bytes, to_be_bytes);
for_primitive!(i64, BigEndian, from_be_bytes, to_be_bytes);
for_primitive!(u128, BigEndian, from_be_bytes, to_be_bytes);
for_primitive!(i128, BigEndian, from_be_bytes, to_be_bytes);
for_primitive!(f32, BigEndian, from_be_bytes, to_be_bytes);
for_primitive!(f64, BigEndian, from_be_bytes, to_be_bytes);

for_primitive!(u16, LittleEndian, from_le_bytes, to_le_bytes);
