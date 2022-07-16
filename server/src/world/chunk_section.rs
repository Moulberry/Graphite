use binary::{
    slice_serializable_composite,
    slice_serialization::{BigEndian, SliceSerializable},
};

use super::paletted_container::PalettedContainer;

slice_serializable_composite! {
    ChunkSection,
    non_air_blocks: u16 as BigEndian,
    block_palette: PalettedContainer,
    biome_palette: PalettedContainer
}

/*#[derive(Clone, Copy)]
pub struct ChunkSection {
    non_air_blocks: u16,
    block_palette: PalettedContainer,
    biome_palette: PalettedContainer
}

impl SliceSerializable<'_> for ChunkSection {
    type RefType = Self;

    #[inline(always)]
    fn maybe_deref(t: &Self) -> Self::RefType {
        *t
    }

    fn read(bytes: &mut &[u8]) -> anyhow::Result<Self> {
        todo!()
    }

    unsafe fn write(bytes: &mut [u8], data: Self::RefType) -> &mut [u8] {
        todo!()
    }

    fn get_write_size(data: Self::RefType) -> usize {
        todo!()
    }
}*/
