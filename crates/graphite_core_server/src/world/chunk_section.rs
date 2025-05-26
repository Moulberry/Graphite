

use graphite_binary::slice_serialization::{BigEndian, SliceSerializable};

use super::paletted_container::{BiomePalettedContainer, BlockPalettedContainer};

#[derive(Clone, Debug)]
pub struct ChunkSection {
    non_air_blocks: u16,
    block_palette: BlockPalettedContainer,
    biome_palette: BiomePalettedContainer,
    pub block_light: Option<Box<[u8]>>, // Length 2048
    pub sky_light: Option<Box<[u8]>>, // Length 2048
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
            biome_palette,
            block_light: None,
            sky_light: None,
        }
    }

    pub fn new_empty() -> Self {
        Self {
            non_air_blocks: 0,
            block_palette: BlockPalettedContainer::Single(0),
            biome_palette: BiomePalettedContainer::Single(0),
            block_light: None,
            sky_light: None,
        }
    }

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

    pub fn maybe_get_single_block(&self) -> Option<u16> {
        if self.non_air_blocks == 0 {
            return Some(0);
        }

        self.get_block_palette().maybe_get_single()
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

    pub fn rotate_clockwise<F>(&self, rotation: F) -> Self
        where F: Fn(u16) -> u16
    {
        let empty: Option<Box<dyn Fn(u16) -> u16>> = None;

        Self {
            non_air_blocks: self.non_air_blocks,
            block_palette: self.block_palette.rotate_clockwise(Some(rotation)),
            biome_palette: self.biome_palette.rotate_clockwise(empty),
            block_light: self.block_light.as_ref().map(rotate_light_clockwise),
            sky_light: self.sky_light.as_ref().map(rotate_light_clockwise),
        }
    }
}

fn rotate_light_clockwise(light: &Box<[u8]>) -> Box<[u8]> {
    // XZY order

    if cfg!(debug_assertions) {
        panic!("untested, so if I happen to use this code, test it!");
    }

    let mut rotated = Box::new([0_u8; 2048]);

    for y in 0..16 {
        for z in 0..16 {
            let new_x = 15 - z;

            for x in 0..8 {
                let new_z1 = x*2;
                let new_z2 = x*2+1;

                let from_index = (y << 7) | (z << 3) | x;

                let from_value = light[from_index];

                let light1 = from_value & 0xF;
                let light2 = (from_value >> 4) & 0xF;
        
                if new_x & 1 == 1 {
                    rotated[(y << 7) | (new_z1 << 3) | (new_x >> 1)] |= light1 << 4;
                    rotated[(y << 7) | (new_z2 << 3) | (new_x >> 1)] |= light2 << 4;
                } else {
                    rotated[(y << 7) | (new_z1 << 3) | (new_x >> 1)] |= light1;
                    rotated[(y << 7) | (new_z2 << 3) | (new_x >> 1)] |= light2;
                }
            }
        }   
    }

    rotated
}

impl<'r, 'd: 'r> SliceSerializable<'r, 'd> for ChunkSection {
    type CopyType = &'r Self;

    fn as_copy_type(t: &'r Self) -> Self::CopyType {
        t
    }

    fn read(_: &mut &[u8]) -> anyhow::Result<Self> {
        unimplemented!();
    }

    unsafe fn write<'b>(mut bytes: &'b mut [u8], data: &'r Self) -> &'b mut [u8] {
        bytes = <BigEndian as SliceSerializable<u16>>::write(bytes, data.non_air_blocks);
        bytes = <BlockPalettedContainer as SliceSerializable>::write(bytes, &data.block_palette);
        bytes = <BiomePalettedContainer as SliceSerializable>::write(bytes, &data.biome_palette);
        bytes
    }

    fn get_write_size(data: &'r Self) -> usize {
        <BigEndian as SliceSerializable<u16>>::get_write_size(data.non_air_blocks) + 
        <BlockPalettedContainer as SliceSerializable>::get_write_size(&data.block_palette) + 
        <BiomePalettedContainer as SliceSerializable>::get_write_size(&data.biome_palette)
    }
}
