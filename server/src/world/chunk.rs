use std::borrow::Cow;

use binary::{
    slice_serializable_composite,
    slice_serialization::{BigEndian, GreedyBlob, SliceSerializable},
};
use legion::Entity;
use net::{network_buffer::WriteBuffer, packet_helper};
use protocol::play::server::{self, ChunkBlockData, ChunkLightData};
use slab::Slab;

use super::{chunk_section::ChunkSection, paletted_container::{BlockPalettedContainer, BiomePalettedContainer}};

pub struct Chunk {
    block_sections: Vec<ChunkSection>,

    valid_cache: bool,
    cached_block_data: WriteBuffer,
    cached_light_data: WriteBuffer,

    pub(crate) viewable_buffer: WriteBuffer,
    pub(crate) entities: Slab<Entity>,
    pub(crate) player_count: usize,
    pub(crate) spot_buffer: WriteBuffer,
}

impl Chunk {
    pub const SECTION_BLOCK_WIDTH_F: f32 = 16.0;
    pub const SECTION_BLOCK_WIDTH_I: usize = 16;

    #[inline(always)]
    pub fn to_chunk_coordinate(f: f32) -> i32 {
        (f / Chunk::SECTION_BLOCK_WIDTH_F).floor() as i32
    }

    pub fn copy_into_spot_buffer(&mut self, bytes: &[u8]) {
        if self.player_count > 0 {
            self.spot_buffer.copy_from(bytes);
        }
    }

    pub fn new(empty: bool) -> Self {
        // Setup default block sections
        let mut block_sections = Vec::new();
        for i in 0..24 {
            if i < 18 && !empty {
                let chunk_section = ChunkSection::new(
                    16 * 16 * 16,
                    BlockPalettedContainer::filled(1),
                    BiomePalettedContainer::filled(1)
                );

                block_sections.push(chunk_section);
            } else {
                let chunk_section = ChunkSection::new(
                    0,
                    BlockPalettedContainer::filled(0),
                    BiomePalettedContainer::filled(1)
                );

                block_sections.push(chunk_section);
            }
        }

        Self {
            block_sections,
            valid_cache: false,
            cached_block_data: WriteBuffer::new(),
            cached_light_data: WriteBuffer::new(),
            viewable_buffer: WriteBuffer::new(),
            entities: Slab::new(),
            player_count: 0,
            spot_buffer: WriteBuffer::new(),
        }
    }

    pub fn fill_blocks(&mut self, index: usize, block: u16) {
        if self.block_sections[index].fill_blocks(block) {
            self.invalidate_cache();
        }
    }

    pub fn set_block(&mut self, x: u8, y: usize, z: u8, block: u16) {
        let index = y / Self::SECTION_BLOCK_WIDTH_I + 4;  // temp: remove + 4 when world limit is set to y = 0
        let section_y = y % Self::SECTION_BLOCK_WIDTH_I;
        if self.block_sections[index].set_block(x, section_y as _, z, block) {
            self.invalidate_cache();
        }
    }

    fn invalidate_cache(&mut self) {
        self.valid_cache = false;
    }

    fn compute_cache(&mut self) {
        self.valid_cache = true;

        // Write chunk data
        let mut chunk_data = WriteBuffer::new();
        for block_section in &self.block_sections {
            packet_helper::write_slice_serializable(&mut chunk_data, block_section);
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

        self.cached_block_data.reset();
        net::packet_helper::write_slice_serializable(
            &mut self.cached_block_data,
            &chunk_block_data,
        );

        self.cached_light_data.reset();
        net::packet_helper::write_slice_serializable(
            &mut self.cached_light_data,
            &chunk_light_data,
        );
    }

    pub fn write(
        &mut self,
        write_buffer: &mut WriteBuffer,
        chunk_x: i32,
        chunk_z: i32,
    ) -> anyhow::Result<()> {
        if !self.valid_cache {
            self.compute_cache();
        }

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
