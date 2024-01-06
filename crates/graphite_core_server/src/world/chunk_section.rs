use std::alloc::Layout;

use graphite_binary::slice_serialization::{BigEndian, SliceSerializable};

use super::paletted_container::{BiomePalettedContainer, BlockPalettedContainer};

#[derive(Clone, Debug)]
pub struct ChunkSection {
    non_air_blocks: u16,
    block_palette: BlockPalettedContainer,
    biome_palette: BiomePalettedContainer,
}

impl ChunkSection {
    pub fn fill_blocks(&mut self, block: u16) -> bool {
        if block == 0 {
            self.non_air_blocks = 0;
        } else {
            self.non_air_blocks = 16 * 16 * 16;
        }

        self.get_block_palette_mut().fill(block)
    }

    pub fn set_block(&mut self, x: u8, y: u8, z: u8, block: u16) -> Option<u16> {
        if let Some(previous) = self.get_block_palette_mut().set(x, y, z, block) {
            debug_assert_ne!(previous, block);

            // Update non_air_block count
            if previous == 0 {
                self.non_air_blocks += 1;
            } else if block == 0 {
                self.non_air_blocks -= 1;
            }

            Some(previous)
        } else {
            None
        }
    }

    pub fn get_non_air_count(&self) -> u16 {
        self.non_air_blocks
    }

    pub fn get_block(&self, x: u8, y: u8, z: u8) -> u16 {
        self.get_block_palette().get(x, y, z)
    }

    pub fn get_block_palette(&self) -> &BlockPalettedContainer {
        &self.block_palette
    }

    fn get_block_palette_mut(&mut self) -> &mut BlockPalettedContainer {
        &mut self.block_palette
    }
}

impl ChunkSection {
    pub fn new(
        non_air_blocks: u16,
        block_palette: BlockPalettedContainer,
        biome_palette: BiomePalettedContainer,
    ) -> Self {
        Self {
            non_air_blocks,
            block_palette,
            biome_palette
        }
    }

    pub fn new_empty() -> Self {
        Self {
            non_air_blocks: 0,
            block_palette: BlockPalettedContainer::Single(0),
            biome_palette: BiomePalettedContainer::Single(0)
        }
    }
}

impl<'a> SliceSerializable<'a> for ChunkSection {
    type CopyType = &'a Self;

    fn as_copy_type(t: &'a Self) -> Self::CopyType {
        t
    }

    fn read(_: &mut &[u8]) -> anyhow::Result<Self> {
        unimplemented!();
    }

    unsafe fn write<'b>(mut bytes: &'b mut [u8], data: &'a Self) -> &'b mut [u8] {
        bytes = <BigEndian as SliceSerializable<u16>>::write(bytes, data.non_air_blocks);
        bytes = <BlockPalettedContainer as SliceSerializable>::write(bytes, &data.block_palette);
        bytes = <BiomePalettedContainer as SliceSerializable>::write(bytes, &data.biome_palette);
        bytes
    }

    fn get_write_size(data: &'a Self) -> usize {
        <BigEndian as SliceSerializable<u16>>::get_write_size(data.non_air_blocks) + 
        <BlockPalettedContainer as SliceSerializable>::get_write_size(&data.block_palette) + 
        <BiomePalettedContainer as SliceSerializable>::get_write_size(&data.biome_palette)
    }
}
