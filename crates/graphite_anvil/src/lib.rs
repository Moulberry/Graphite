use std::io::{Cursor, Read};

use graphite_binary::{nbt::{TAG_COMPOUND_ID, NBT}, slice_serialization::*};

use byteorder::{ByteOrder, ReadBytesExt};
use graphite_binary::slice_serialization::BigEndian;
use graphite_core_server::world::{
    chunk_section::ChunkSection,
    paletted_container::{BiomePalettedContainer, BlockPalettedContainer, self},
    ChunkList,
};
use graphite_mc_constants::block::Block;

pub struct ChunkCoord {
    x: isize,
    z: isize,
}

impl ChunkCoord {
    pub fn new(x: isize, z: isize) -> Self {
        Self {
            x,
            z
        }
    }
}

pub struct AnvilWorld {
    size_x: usize,
    size_y: usize,
    size_z: usize,
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

pub fn load_anvil(
    min: ChunkCoord,
    chunk_height: usize,
    max: ChunkCoord,
    folder: include_dir::Dir,
) -> Result<AnvilWorld, ()> { // todo: return errors
    assert!(min.x <= max.x);
    assert!(min.z <= max.z);

    let size_x = (max.x - min.x + 1) as usize;
    let size_y = chunk_height;
    let size_z = (max.z - min.z + 1) as usize;

    let mut chunks: Vec<Vec<ChunkSection>> = vec![vec![ChunkSection::new_empty(); size_y]; size_x * size_z];

    let min_region_x = min.x >> 5;
    let min_region_z = min.z >> 5;
    let max_region_x = max.x >> 5;
    let max_region_z = max.z >> 5;

    for region_x in min_region_x..=max_region_x {
        for region_z in min_region_z..=max_region_z {
            let file = folder.get_file(format!("r.{}.{}.mca", region_x, region_z));
            let Some(file) = file else {
                // File doesn't exist
                for chunk_x in region_x*32..region_x*32+32 {
                    for chunk_z in region_z*32..region_z*32+32 {
                        if (min.x..=max.x).contains(&chunk_x) && (min.z..=max.z).contains(&chunk_z) {
                            let rel_x = (chunk_x - min.x) as usize;
                            let rel_z = (chunk_z - min.z) as usize;
                            let chunk_index = rel_x + rel_z * size_x;

                            chunks[chunk_index].extend((0..=chunk_height).map(|_| ChunkSection::new_empty()));
                        }
                    }
                }
                continue;
            };

            let region = file.contents();

            let mut cursor = Cursor::new(region);

            for chunk_x in region_x*32..region_x*32+32 {
                for chunk_z in region_z*32..region_z*32+32 {
                    if !(min.x..=max.x).contains(&chunk_x) || !(min.z..=max.z).contains(&chunk_z) {
                        continue;
                    }

                    let rel_x = (chunk_x - min.x) as usize;
                    let rel_z = (chunk_z - min.z) as usize;
                    let chunk_index = rel_x + rel_z * size_x;
                    
                    let location_offset = 4 * ((chunk_x & 31) + (chunk_z & 31) * 32);
                    cursor.set_position(location_offset as u64);

                    let offset = cursor.read_u24::<byteorder::BigEndian>().unwrap();
                    let _sector_count = cursor.read_u8().unwrap();

                    let start = (offset * 4096) as usize;
                    let mut region_bytes = &region[start..start + 5];
        
                    let length: i32 = BigEndian::read(&mut region_bytes).unwrap();
                    let compression_type: u8 = Single::read(&mut region_bytes).unwrap();
        
                    if compression_type != 2 {
                        panic!("Only Zlib compression is supported");
                    }
        
                    let region_bytes = &region[start + 5..start + 5 + (length as usize) - 1];
        
                    let mut decompress_buf = vec![];
        
                    let mut decoder = flate2::bufread::ZlibDecoder::new(region_bytes);
                    decoder.read_to_end(&mut decompress_buf).unwrap();
        
                    let mut decompressed_slice = decompress_buf.as_slice();
        
                    let chunk_data = graphite_binary::nbt::decode::read_named(&mut decompressed_slice).unwrap();
                    read_sections_into(&chunk_data, &mut chunks[chunk_index]);
                }
            }
        }
    }

    Ok(AnvilWorld {
        size_x,
        size_y,
        size_z,
        chunks,
    })
}

fn read_sections_into(chunk_data: &NBT, section_chunks: &mut Vec<ChunkSection>) {
    let Some(sections) = chunk_data.find_list("sections", TAG_COMPOUND_ID) else {
        return;
    };

    for (section_index, section) in sections.iter().enumerate() {
        if section_index >= section_chunks.len() {
            break;
        }

        let section = section.as_compound().unwrap();
        
        let Some(block_states) = section.find_compound("block_states") else {
            continue;
        };
        
        let Some(palette) = block_states.find_list("palette", TAG_COMPOUND_ID) else {
            continue;
        };

        let mut palette_vec: Vec<u16> = Vec::with_capacity(palette.len());

        for palette_entry in palette.iter() {
            let palette_entry = palette_entry.as_compound().unwrap();

            let Some(name) = palette_entry.find_string("Name") else {
                continue;
            };
            
            let mut id = graphite_mc_constants::block::string_to_u16(name).unwrap_or(0);

            if let Some(properties) = palette_entry.find_compound("Properties") {
                if !properties.is_empty() {
                    let mut block: Block = id.try_into().unwrap();
                    for (key, value) in properties.entries() {
                        if let Some(value_str) = value.as_string() {
                            block =
                                block.set_property(key, value_str).unwrap_or(block);
                        }
                    }
                    id = block.to_id();
                }
            }

            palette_vec.push(id);
        }

        let Some(data) = block_states.find_long_array("data") else {
            if !palette_vec.is_empty() {
                let non_air_blocks = if palette_vec[0] == 0 { 0 } else { 4096 };
                section_chunks[section_index] = ChunkSection::new(
                    non_air_blocks,
                    BlockPalettedContainer::filled(palette_vec[0]),
                    BiomePalettedContainer::filled(0),
                );
            }
            continue;
        };

        if let Some((non_air_blocks, block_palette)) = create_palette(palette_vec, data) {
            section_chunks[section_index] = ChunkSection::new(
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