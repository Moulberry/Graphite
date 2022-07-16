use binary::slice_serialization::{self, Single, SliceSerializable};

#[derive(Debug, Clone, Copy)]
pub enum PalettedContainer {
    Single(i32),
}

impl SliceSerializable<'_> for PalettedContainer {
    type RefType = Self;

    fn maybe_deref(t: &Self) -> Self::RefType {
        *t
    }

    fn read(bytes: &mut &[u8]) -> anyhow::Result<Self> {
        unimplemented!()
    }

    unsafe fn write(mut bytes: &mut [u8], data: Self) -> &mut [u8] {
        match data {
            Self::Single(value) => {
                bytes = <Single as SliceSerializable<'_, u8>>::write(bytes, 0); // 0 bits per block
                bytes = slice_serialization::VarInt::write(bytes, value);
                <Single as SliceSerializable<'_, u8>>::write(bytes, 0) // 0 size array
            }
        }
    }

    fn get_write_size(data: Self) -> usize {
        1 + match data {
            Self::Single(value) => slice_serialization::VarInt::get_write_size(value),
        }
    }
}
