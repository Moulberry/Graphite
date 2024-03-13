use std::io::{Cursor, Read};

use graphite_binary::{nbt::NBT, slice_serialization::*};

use byteorder::ReadBytesExt;
use graphite_binary::slice_serialization::BigEndian;

mod world;
mod entity;
pub use world::load_anvil_world;
pub use entity::load_anvil_entities;

#[derive(Clone, Copy)]
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

fn load_anvil(
    min: ChunkCoord,
    max: ChunkCoord,
    folder: include_dir::Dir,
    mut handle_chunk: impl FnMut(isize, isize, NBT)
) {
    assert!(min.x <= max.x);
    assert!(min.z <= max.z);

    let min_region_x = min.x >> 5;
    let min_region_z = min.z >> 5;
    let max_region_x = max.x >> 5;
    let max_region_z = max.z >> 5;

    for region_x in min_region_x..=max_region_x {
        for region_z in min_region_z..=max_region_z {
            let file = folder.get_file(format!("r.{}.{}.mca", region_x, region_z));
            let Some(file) = file else {
                continue;
            };

            let region = file.contents();
            if region.is_empty() {
                continue;
            }

            let mut cursor = Cursor::new(region);

            for chunk_x in region_x*32..region_x*32+32 {
                for chunk_z in region_z*32..region_z*32+32 {
                    if !(min.x..=max.x).contains(&chunk_x) || !(min.z..=max.z).contains(&chunk_z) {
                        continue;
                    }
                    
                    let location_offset = 4 * ((chunk_x & 31) + (chunk_z & 31) * 32);
                    cursor.set_position(location_offset as u64);

                    let offset = cursor.read_u24::<byteorder::BigEndian>().unwrap();
                    let sector_count = cursor.read_u8().unwrap();

                    if offset == 0 || sector_count == 0 {
                        continue;
                    }

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
                    handle_chunk(chunk_x, chunk_z, chunk_data);
                }
            }
        }
    }
}