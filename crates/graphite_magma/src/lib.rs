use anyhow::bail;
use graphite_binary::slice_serialization::BigEndian;

use graphite_binary::slice_serialization::LittleEndian;
use graphite_binary::slice_serialization::Single;
use graphite_binary::slice_serialization::SliceSerializable;
use graphite_binary::slice_serialization::VarInt;
use graphite_binary::varint;
use byteorder::ByteOrder;
use bytes::BufMut;

use graphite_server::world::chunk::Chunk;
use graphite_server::world::chunk_section::ChunkSection;
use graphite_server::world::paletted_container::BiomePalettedContainer;
use graphite_server::world::paletted_container::BlockPalettedContainer;
use graphite_server::world::{chunk_list::ChunkGrid, paletted_container::PalettedContainer};
use thiserror::Error;

const MAGIC: u16 = 0x8C73;
const DATA_VERSION: u32 = 760;

#[derive(Debug, Error)]
pub enum MagmaEncodeError {
    #[error("the reported dimensions were incorrect")]
    WrongSize,
    #[error("the block counts of an array palette did not sum to 4096")]
    InvalidPaletteCount
}

pub fn to_magma(chunks: &ChunkGrid, custom_data: u32) -> Result<Vec<u8>, MagmaEncodeError> {
    let mut out = Vec::new();

    // Magic
    out.put_u16(MAGIC);

    // Dimensions
    let (size_x, size_y, size_z) = chunks.dimensions();
    varint::encode::extend_i32(&mut out, size_x as i32);
    varint::encode::extend_i32(&mut out, size_y as i32);
    varint::encode::extend_i32(&mut out, size_z as i32);

    // Data Version
    varint::encode::extend_i32(&mut out, DATA_VERSION as i32);

    // Custom Data
    varint::encode::extend_i32(&mut out, custom_data as i32);

    for (x, z, chunk) in chunks.enumerate() {
        let sections = chunk.get_block_sections();

        if x >= size_x || z >= size_z || sections.len() != size_y {
            return Err(MagmaEncodeError::WrongSize)
        }
        
        for section in sections {
            let _flag_index = out.len();

            write_block_palette(section.get_block_palette(), section.get_non_air_count(), &mut out)?;

            // todo: biomes
        }
    }

    Ok(out)
}

pub fn from_magma(mut bytes: &[u8]) -> anyhow::Result<(ChunkGrid, u32)> {
    let magic: u16 = BigEndian::read(&mut bytes)?;
    if magic != MAGIC {
        bail!("file is not a magma world format");
    }

    let size_x: usize = VarInt::read(&mut bytes)?;
    let size_y: usize = VarInt::read(&mut bytes)?;
    let size_z: usize = VarInt::read(&mut bytes)?;

    let data_version: u32 = VarInt::read(&mut bytes)?;
    if data_version != DATA_VERSION {
        bail!("wrong data version");
    }

    let custom_data: i32 = VarInt::read(&mut bytes)?;
    let custom_data = custom_data as u32;

    let mut chunks = Vec::new();
    for z in 0..size_z {
        for x in 0..size_x {
            let mut sections = Vec::new();
            for _ in 0..size_y {
                let flags: u8 = Single::read(&mut bytes)?;
                
                let block_flags = flags & 0b11;
                let (non_air_blocks, block_palette) = match block_flags {
                     0 => {
                        (0, BlockPalettedContainer::filled(0))
                     }
                     1 => {
                        let v: u16 = VarInt::read(&mut bytes)?;
                        let non_air_blocks = if v == 0 { 0 } else { 16*16*16 };
                        (non_air_blocks, BlockPalettedContainer::filled(v))
                     }
                     2 => {
                        let non_air_count: u16 = LittleEndian::read(&mut bytes)?;

                        let palette_size: u8 = Single::read(&mut bytes)?;

                        if palette_size > 16 {
                            bail!("palette size exceeds maximum")
                        }

                        let mut palette: heapless::Vec<(u16, usize), 16> = heapless::Vec::new();
                        for _ in 0..palette_size {
                            let block: u16 = VarInt::read(&mut bytes)?;
                            let count: usize = VarInt::read(&mut bytes)?;
                            palette.push((block, count)).unwrap();
                        }

                        if bytes.len() < 2048 {
                            bail!("not enough bytes");
                        }

                        let (contents, remaining) = bytes.split_at(2048);
                        bytes = remaining;

                        // todo: validate requirements of BlockPalettedContainer::array
                        unsafe {
                            (non_air_count, BlockPalettedContainer::array(palette, contents.try_into().unwrap()))
                        }
                     }
                     3 => {
                        let non_air_count: u16 = LittleEndian::read(&mut bytes)?;

                        let mut contents = [0_u64; 1024];

                        let (byte_contents, remaining) = bytes.split_at(8192);
                        bytes = remaining;

                        byteorder::LittleEndian::read_u64_into(byte_contents, &mut contents);

                        (non_air_count, BlockPalettedContainer::direct(contents))
                     },
                     _ => unreachable!()
                };

                let chunk_section = ChunkSection::new(non_air_blocks, block_palette, 
                    BiomePalettedContainer::filled(0));
                sections.push(chunk_section);
            }
            chunks.push(Chunk::new(sections));
        }
    }

    let chunk_list = ChunkGrid::new(chunks, size_x, size_y, size_z);
    Ok((chunk_list, custom_data))
}

fn write_block_palette(palette: &BlockPalettedContainer, non_air_count: u16, out: &mut Vec<u8>) -> Result<(), MagmaEncodeError> {
    match palette {
        PalettedContainer::Single(v) => {
            if *v == 0 {
                out.push(0);
            } else {
                out.push(1);
                varint::encode::extend_i32(out, *v as i32);
            }
        },
        PalettedContainer::Array(array) => {
            out.push(2);
            out.put_u16_le(non_air_count);

            let mut total_count = 0;
            let mut end = 0;
            for (index, (_, count)) in array.indices.iter().enumerate() {
                if *count > 0 {
                    end = index + 1;
                    total_count += *count;
                }
            }

            if end == 0 || total_count != 4096 {
                return Err(MagmaEncodeError::InvalidPaletteCount)
            }

            out.push(end as u8);
            for index in 0..end {
                let (block, count) = array.indices[index];
                if count > 0 {
                    varint::encode::extend_i32(out, block as i32);
                    varint::encode::extend_i32(out, count as i32);
                } else {
                    // We could "defragment" the palette to avoid this, but it isn't worth it
                    out.push(0);
                    out.push(0);
                }
            }

            out.put_slice(&array.contents);
        },
        PalettedContainer::Direct(direct) => {
            out.push(3);
            out.put_u16_le(non_air_count);

            let direct_bytes = direct.contents.len()*8;
            let direct_index = out.len();
            out.resize(out.len() + direct_bytes, 0);
            let dst = &mut out[direct_index..];

            byteorder::LittleEndian::write_u64_into(direct.contents.as_slice(), dst);
        },
    }
    Ok(())
}