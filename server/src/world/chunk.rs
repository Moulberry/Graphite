use binary::{
    slice_serializable_composite,
    slice_serialization::{BigEndian, GreedyBlob, SliceSerializable},
};
use legion::Entity;
use net::{network_buffer::WriteBuffer, packet_helper};
use protocol::play::server::{self, ChunkBlockData, ChunkLightData};
use slab::Slab;

use super::{chunk_section::ChunkSection, paletted_container::PalettedContainer};

pub struct Chunk {
    cached_block_data: WriteBuffer,
    cached_light_data: WriteBuffer,
    pub(crate) viewable_buffer: WriteBuffer,
    pub(crate) entities: Slab<Entity>,
    pub(crate) player_count: usize,
    pub(crate) spot_buffer: WriteBuffer
}

impl Chunk {
    pub const SECTION_BLOCK_WIDTH: f32 = 16.0;

    #[inline(always)]
    pub fn to_chunk_coordinate(f: f32) -> i32 {
        (f / Chunk::SECTION_BLOCK_WIDTH).floor() as i32
    }

    pub fn copy_into_spot_buffer(&mut self, bytes: &[u8]) {
        if self.player_count > 0 {
            self.spot_buffer.copy_from(bytes);
        }
    }

    pub fn new(empty: bool) -> Self {
        let mut chunk_data = WriteBuffer::new();
        for i in 0..24 {
            if i < 18 && !empty {
                let chunk_section = ChunkSection {
                    non_air_blocks: 16 * 16 * 16,
                    block_palette: PalettedContainer::Single(1), // stone
                    biome_palette: PalettedContainer::Single(1),
                };

                packet_helper::write_slice_serializable(&mut chunk_data, &chunk_section);
            } else {
                let chunk_section = ChunkSection {
                    non_air_blocks: 0,
                    block_palette: PalettedContainer::Single(0), // air
                    biome_palette: PalettedContainer::Single(1),
                };

                packet_helper::write_slice_serializable(&mut chunk_data, &chunk_section);
            }
        }

        let chunk_block_data = ChunkBlockData {
            heightmaps: &[10, 0, 0, 0],
            data: chunk_data.get_written(),
            block_entity_count: 0,
            trust_edges: true,
        };

        let chunk_light_data = ChunkLightData {
            sky_light_mask: vec![],
            block_light_mask: vec![],
            empty_sky_light_mask: vec![],
            empty_block_light_mask: vec![],
            sky_light_entries: vec![],
            block_light_entries: vec![],
        };

        let mut cached_block_data = WriteBuffer::new();
        net::packet_helper::write_slice_serializable(&mut cached_block_data, &chunk_block_data);

        let mut cached_light_data = WriteBuffer::new();
        net::packet_helper::write_slice_serializable(&mut cached_light_data, &chunk_light_data);

        Self {
            cached_block_data,
            cached_light_data,
            viewable_buffer: WriteBuffer::new(),
            entities: Slab::new(),
            player_count: 0,
            spot_buffer: WriteBuffer::new()
        }
    }

    pub fn write(
        &self,
        write_buffer: &mut WriteBuffer,
        chunk_x: i32,
        chunk_z: i32,
    ) -> anyhow::Result<()> {
        let composite = DirectLevelChunkWithLight {
            chunk_x,
            chunk_z,
            chunk_block_data: self.cached_block_data.get_written(),
            chunk_light_data: self.cached_light_data.get_written(),
        };

        let packet_id = server::PacketId::LevelChunkWithLight as u8;
        net::packet_helper::write_custom_packet(write_buffer, packet_id, &composite)
    }
}

slice_serializable_composite! {
    DirectLevelChunkWithLight<'a>,
    chunk_x: i32 as BigEndian,
    chunk_z: i32 as BigEndian,
    chunk_block_data: &'a [u8] as GreedyBlob,
    chunk_light_data: &'a [u8] as GreedyBlob,
}
