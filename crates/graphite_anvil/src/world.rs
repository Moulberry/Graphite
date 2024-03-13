use byteorder::ByteOrder;
use graphite_binary::nbt::{NBT, TAG_COMPOUND_ID};
use graphite_core_server::world::{chunk_section::ChunkSection, paletted_container::{self, BiomePalettedContainer, BlockPalettedContainer}, ChunkList};
use graphite_mc_constants::block::BlockState;

use crate::ChunkCoord;

pub struct AnvilWorld {
    pub(crate) size_x: usize,
    pub(crate) size_y: usize,
    pub(crate) size_z: usize,
    pub(crate) min_chunk_x: isize,
    pub(crate) min_chunk_y: isize,
    pub(crate) min_chunk_z: isize,
    chunks: Vec<Vec<ChunkSection>>,
}

impl From<&AnvilWorld> for ChunkList {
    fn from(value: &AnvilWorld) -> Self {
        Self {
            size_x: value.size_x,
            size_y: value.size_y,
            size_z: value.size_z,
            chunks: value.chunks.clone(),
        }
    }
}

impl From<AnvilWorld> for ChunkList {
    fn from(value: AnvilWorld) -> Self {
        Self {
            size_x: value.size_x,
            size_y: value.size_y,
            size_z: value.size_z,
            chunks: value.chunks,
        }
    }
}

pub fn load_anvil_world(
    min: ChunkCoord,
    chunk_height: usize,
    max: ChunkCoord,
    folder: include_dir::Dir,
) -> Result<AnvilWorld, ()> { // todo: return errors
    let size_x = (max.x - min.x + 1) as usize;
    let size_y = chunk_height;
    let size_z = (max.z - min.z + 1) as usize;

    let mut chunks: Vec<Vec<ChunkSection>> = vec![vec![ChunkSection::new_empty(); size_y]; size_x * size_z];

    let mut world_min_y: Option<i32> = None;

    super::load_anvil(min, max, folder, |chunk_x, chunk_z, chunk_data| {
        let Some(min_y) = chunk_data.find_int("yPos") else {
            return;
        };
        if let Some(world_min_y) = world_min_y {
            if *min_y != world_min_y {
                panic!("inconsistent yPos between chunks")
            }
        } else {
            world_min_y = Some(*min_y);
        }

        let rel_x = (chunk_x - min.x) as usize;
        let rel_z = (chunk_z - min.z) as usize;
        let chunk_index = rel_x + rel_z * size_x;
        read_sections_into(&chunk_data, *min_y, &mut chunks[chunk_index]);
    });

    Ok(AnvilWorld {
        size_x,
        size_y,
        size_z,
        min_chunk_x: min.x,
        min_chunk_y: world_min_y.unwrap_or(0) as isize,
        min_chunk_z: min.z,
        chunks,
    })
}

fn read_sections_into(chunk_data: &NBT, min_y: i32, section_chunks: &mut Vec<ChunkSection>) {
    let Some(sections) = chunk_data.find_list("sections", TAG_COMPOUND_ID) else {
        return;
    };

    for (_, section) in sections.iter().enumerate() {
        let section = section.as_compound().unwrap();

        let Some(section_y) = section.find_byte("Y") else {
            continue;
        };
        let section_y = *section_y as i32;
        let section_index = section_y - min_y;

        if section_index < 0 || section_index >= section_chunks.len() as _ {
            continue;
        }
        
        let Some(block_states) = section.find_compound("block_states") else {
            continue;
        };
        
        let Some(palette) = block_states.find_list("palette", TAG_COMPOUND_ID) else {
            continue;
        };

        let mut palette_vec: Vec<u16> = Vec::with_capacity(palette.len());

        for palette_entry in palette.iter() {
            let id = graphite_mc_constants::block::parse_block_state(palette_entry.as_compound().unwrap());
            palette_vec.push(id);
        }

        let Some(data) = block_states.find_long_array("data") else {
            if !palette_vec.is_empty() {
                let non_air_blocks = if palette_vec[0] == 0 { 0 } else { 4096 };
                section_chunks[section_index as usize] = ChunkSection::new(
                    non_air_blocks,
                    BlockPalettedContainer::filled(palette_vec[0]),
                    BiomePalettedContainer::filled(0),
                );
            }
            continue;
        };

        if let Some((non_air_blocks, block_palette)) = create_palette(palette_vec, data) {
            section_chunks[section_index as usize] = ChunkSection::new(
                non_air_blocks,
                block_palette,
                BiomePalettedContainer::filled(0),
            );
        }
    }
}

fn create_palette(palette_vec: Vec<u16>, data: &Vec<i64>) -> Option<(u16, BlockPalettedContainer)> {
    if palette_vec.len() > 16 {
        Some(create_direct_palette(palette_vec, data))
    } else if palette_vec.len() > 1 {
        Some(create_array_palette(palette_vec, data))
    } else if palette_vec.len() == 1 {
        let non_air_blocks = if palette_vec[0] == 0 { 0 } else { 4096 };
        Some((non_air_blocks, BlockPalettedContainer::filled(palette_vec[0])))
    } else {
        None
    }
}

fn create_array_palette(palette_vec: Vec<u16>, data: &Vec<i64>) -> (u16, BlockPalettedContainer) {
    let mut array_palette: heapless::Vec<(u16, usize), 16> = heapless::Vec::new();
    for block in palette_vec {
        array_palette.push((block, 0)).unwrap();
    }

    let mut array_data = [0_u8; 2048];
    byteorder::BigEndian::write_i64_into(data, &mut array_data);

    let mut non_air_blocks = 0;

    for byte in array_data {
        for nibble_index in 0..2 {
            let nibble = (byte >> (nibble_index*4)) & 0xF;
            let (block, count) = &mut array_palette[nibble as usize];

            *count += 1;
            if *block != 0 {
                non_air_blocks += 1;
            }
        }
    }

    (non_air_blocks, unsafe { BlockPalettedContainer::array(array_palette, array_data) })
}

fn create_direct_palette(palette_vec: Vec<u16>, data: &Vec<i64>) -> (u16, BlockPalettedContainer) {
    let from_bits_per_block = ((palette_vec.len() - 1).ilog2() + 1) as usize;

    let from_per_array = 64 / from_bits_per_block;
    let from_mask = (1 << from_bits_per_block) - 1;

    let to_bits_per_block = paletted_container::BLOCK_ENTRY_BITS;
    let to_per_array = 64 / to_bits_per_block;
    let to_mask = (1 << to_bits_per_block) - 1;

    let mut non_air_blocks = 0;
    let mut contents = [0_u64; paletted_container::BLOCK_DIRECT_LEN];

    for index in 0..4096 {
        let data_index = index / from_per_array;
        let shift_by = from_bits_per_block * (index % from_per_array);

        let data_value = data[data_index as usize];
        let palette_value = (data_value >> shift_by) & from_mask;

        let block = palette_vec[palette_value as usize];
        if block != 0 {
            non_air_blocks += 1;
        }

        // Put in content!
        let to_data_index = index / to_per_array;
        let to_shift_by = to_bits_per_block * (index % to_per_array);

        let mut content_value = contents[to_data_index];
        content_value &= !(to_mask << to_shift_by);
        content_value |= (block as u64) << to_shift_by;
        contents[to_data_index] = content_value;
    }

    (non_air_blocks, BlockPalettedContainer::direct(contents))
}