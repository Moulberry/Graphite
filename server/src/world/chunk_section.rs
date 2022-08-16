use std::alloc::Layout;

use binary::slice_serialization::{BigEndian, SliceSerializable};

use super::paletted_container::{BiomePalettedContainer, BlockPalettedContainer};

#[derive(Debug)]
pub struct ChunkTemplate {
    non_air_blocks: u16,
    block_palette: BlockPalettedContainer,
    biome_palette: BiomePalettedContainer,
}

impl ChunkTemplate {
    pub fn get(&'static self) -> ChunkSection {
        ChunkSection {
            non_air_blocks: self.non_air_blocks,
            copy_on_write: true,
            block_palette: &self.block_palette as *const _ as *mut _,
            biome_palette: &self.biome_palette as *const _ as *mut _,
        }
    }
}

#[derive(Debug)]
pub struct ChunkSection {
    // If true, mutating methods will clone the palette pointers
    // and then set `copy_on_write` to false
    // i.e. convert self from Borrowed -> Owned
    copy_on_write: bool,

    // Serialized values
    non_air_blocks: u16,
    block_palette: *mut BlockPalettedContainer,
    biome_palette: *mut BiomePalettedContainer,
}

impl ChunkSection {
    pub fn fill_blocks(&mut self, block: u16) -> bool {
        self.get_block_palette_mut().fill(block)
    }

    pub fn set_block(&mut self, x: u8, y: u8, z: u8, block: u16) -> Option<u16> {
        self.get_block_palette_mut().set(x, y, z, block)
    }

    pub fn get_block(&self, x: u8, y: u8, z: u8) -> u16 {
        self.get_block_palette().get(x, y, z)
    }

    fn get_block_palette(&self) -> &BlockPalettedContainer {
        unsafe { &*self.block_palette }
    }

    fn get_block_palette_mut(&mut self) -> &mut BlockPalettedContainer {
        if self.copy_on_write {
            self.perform_copy();
        }
        unsafe { &mut *self.block_palette }
    }

    fn perform_copy(&mut self) {
        unsafe {
            // Copy block_palette
            const BLOCK_LAYOUT: Layout = Layout::new::<BlockPalettedContainer>();
            let new_block_palette = std::alloc::alloc(BLOCK_LAYOUT) as *mut _;
            std::ptr::copy_nonoverlapping(self.block_palette, new_block_palette, 1);
            self.block_palette = new_block_palette;

            // Copy biome_palette
            const BIOME_LAYOUT: Layout = Layout::new::<BiomePalettedContainer>();
            let new_biome_palette = std::alloc::alloc(BIOME_LAYOUT) as *mut _;
            std::ptr::copy_nonoverlapping(self.biome_palette, new_biome_palette, 1);
            self.biome_palette = new_biome_palette;
        }
        self.copy_on_write = false;
    }
}

impl Drop for ChunkSection {
    fn drop(&mut self) {
        if !self.copy_on_write {
            unsafe {
                std::mem::drop(Box::from_raw(self.block_palette));
                std::mem::drop(Box::from_raw(self.biome_palette));
            }
        }
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
            copy_on_write: false,
            block_palette: Box::into_raw(Box::from(block_palette)),
            biome_palette: Box::into_raw(Box::from(biome_palette)),
        }
    }
}

impl<'a> SliceSerializable<'a> for ChunkSection {
    type RefType = &'a Self;

    fn maybe_deref(t: &'a Self) -> Self::RefType {
        t
    }

    fn read(_: &mut &[u8]) -> anyhow::Result<Self> {
        unimplemented!();
    }

    unsafe fn write<'b>(mut bytes: &'b mut [u8], data: &'a Self) -> &'b mut [u8] {
        bytes = <BigEndian as SliceSerializable<'_, u16>>::write(bytes, data.non_air_blocks);
        bytes = <BlockPalettedContainer as SliceSerializable>::write(bytes, &*data.block_palette);
        bytes = <BiomePalettedContainer as SliceSerializable>::write(bytes, &*data.biome_palette);
        bytes
    }

    fn get_write_size(data: &'a Self) -> usize {
        <BigEndian as SliceSerializable<'_, u16>>::get_write_size(data.non_air_blocks)
            + unsafe {
                <BlockPalettedContainer as SliceSerializable>::get_write_size(&*data.block_palette)
                    + <BiomePalettedContainer as SliceSerializable>::get_write_size(
                        &*data.biome_palette,
                    )
            }
    }
}
