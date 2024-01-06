use std::{borrow::Cow, rc::Rc, cell::UnsafeCell};

use graphite_binary::{slice_serialization::*, nbt::CachedNBT};
use graphite_mc_protocol::{play::{clientbound::{ChunkBlockData, ChunkLightData}, self}, IdentifiedPacket, types::BlockPosition};
use graphite_network::PacketBuffer;
use slab::Slab;

use crate::{world::paletted_container::{BlockPalettedContainer, BiomePalettedContainer}, entity::GenericEntity};

use super::chunk_section::ChunkSection;

pub(crate) struct ChunkEntityRef(usize);

pub struct Chunk {
    block_sections: Vec<ChunkSection>,

    pub(crate) entity_viewable: PacketBuffer,
    pub(crate) chunk_viewable: PacketBuffer,

    pub(crate) entities: Slab<Rc<UnsafeCell<dyn GenericEntity>>>,

    valid_cache: bool,
    cached_block_data: PacketBuffer,
    cached_light_data: PacketBuffer,
}

impl Chunk {
    fn invalidate_cache(&mut self) {
        // todo: maybe have more fine-grained invalidation here, not sure if its worth it
        self.valid_cache = false;
    }

    fn compute_cache(&mut self) {
        self.valid_cache = true;

        // Write chunk data
        let mut chunk_data = PacketBuffer::new();
        for block_section in &mut self.block_sections {
            chunk_data.write_raw(block_section);
        }
        
        let chunk_block_data = ChunkBlockData {
            heightmaps: Cow::Owned(CachedNBT::new()),
            data: chunk_data.pop_written(),
            block_entity_count: 0,
            block_entity_data: &[]
        };

        let chunk_light_data = ChunkLightData {
            sky_light_mask: vec![],
            block_light_mask: vec![],
            empty_sky_light_mask: vec![],
            empty_block_light_mask: vec![],
            sky_light_entries: vec![],
            block_light_entries: vec![],
        };

        self.cached_block_data.clear();
        self.cached_block_data.write_raw(&chunk_block_data);

        self.cached_light_data.clear();
        self.cached_light_data.write_raw(&chunk_light_data);
    }

    pub(crate) fn insert_entity(&mut self, entity: Rc<UnsafeCell<dyn GenericEntity>>) -> ChunkEntityRef {
        let idx = self.entities.insert(entity);
        ChunkEntityRef(idx)
    }

    pub(crate) fn clear_viewable_packets(&mut self) {
        self.entity_viewable.clear();
        self.chunk_viewable.clear();
    }

    pub fn add_entity_viewable_packet<'a, I: std::fmt::Debug, T>(&mut self, packet: &'a T)
    where
        T: SliceSerializable<'a, T> + IdentifiedPacket<I> + 'a,
    {
        let _ = self.entity_viewable.write_packet(packet);
    }

    pub fn copy_entity_viewable_packets(&self, buffer: &mut PacketBuffer) {
        buffer.copy_from(&self.entity_viewable);
    }

    pub fn add_chunk_viewable_packet<'a, I: std::fmt::Debug, T>(&mut self, packet: &'a T)
    where
        T: SliceSerializable<'a, T> + IdentifiedPacket<I> + 'a,
    {
        let _ = self.entity_viewable.write_packet(packet);
    }

    pub fn copy_chunk_viewable_packets(&self, buffer: &mut PacketBuffer) {
        buffer.copy_from(&self.chunk_viewable);
    }

    pub fn write_spawn_entities_and_players(&mut self, buffer: &mut PacketBuffer) {
        for (_, entity) in &mut self.entities {
            unsafe { entity.get().as_ref() }.unwrap().write_spawn(buffer);
        }
        // todo: players
    }

    pub fn write_despawn_entities_and_players(&mut self, despawn_list: &mut Vec<i32>, buffer: &mut PacketBuffer) {
        for (_, entity) in &mut self.entities {
            unsafe { entity.get().as_ref() }.unwrap().write_despawn(despawn_list, buffer);
        }
        // todo: players
    }

    pub fn write(
        &mut self,
        packet_buffer: &mut PacketBuffer,
        chunk_x: i32,
        chunk_z: i32,
    ) {
        if !self.valid_cache {
            self.compute_cache();
        }

        let composite = DirectLevelChunkWithLight {
            chunk_x,
            chunk_z,
            chunk_block_data: self.cached_block_data.peek_written(),
            chunk_light_data: self.cached_light_data.peek_written(),
        };

        let packet_id = play::clientbound::PacketId::LevelChunkWithLight as u8;
        let _ = packet_buffer.write_serializable(packet_id, &composite);
    }

    pub fn new(block_sections: Vec<ChunkSection>) -> Self {
        Self {
            block_sections,
            entity_viewable: PacketBuffer::new(),
            chunk_viewable: PacketBuffer::new(),
            entities: Slab::new(),
            valid_cache: false,
            cached_block_data: PacketBuffer::new(),
            cached_light_data: PacketBuffer::new(),
        }
    }

    pub fn new_empty(size_y: usize) -> Self {
        let mut block_sections = Vec::with_capacity(size_y);

        let empty = ChunkSection::new(
            0,
            BlockPalettedContainer::filled(0),
            BiomePalettedContainer::filled(0),
        );

        for _ in 0..size_y {
            block_sections.push(empty.clone());
        }

        Self::new(block_sections)
    }

    pub fn new_default(size_y: usize) -> Self {
        // Setup default block sections
        let mut block_sections = Vec::with_capacity(size_y);

        let filled = ChunkSection::new(
            16 * 16 * 16,
            BlockPalettedContainer::filled(1),
            BiomePalettedContainer::filled(0),
        );
        let empty = ChunkSection::new(
            0,
            BlockPalettedContainer::filled(0),
            BiomePalettedContainer::filled(0),
        );

        for _ in 0..(size_y/3) {
            block_sections.push(filled.clone());
        }
        for _ in (size_y/3)..size_y {
            block_sections.push(empty.clone());
        }

        assert_eq!(block_sections.capacity(), size_y);
        assert_eq!(block_sections.len(), size_y);
        Self::new(block_sections)
    }

    pub fn get_block(&self, x: i32, y: i32, z: i32) -> Option<u16> {
        if y < 0 {
            return None;
        }

        let chunk_y = (y >> 4) as usize;
        if chunk_y >= self.block_sections.len() {
            return None; // out of bounds
        }

        let section = &self.block_sections[chunk_y];
        Some(section.get_block((x & 0xF) as _, (y & 0xF) as _, (z & 0xF) as _))
    }

    pub fn set_block(&mut self, x: i32, y: i32, z: i32, block: u16) {
        let chunk_y = (y >> 4) as usize;
        let previous = self.block_sections[chunk_y].set_block((x & 0xF) as _, (y & 0xF) as _, (z & 0xF) as _, block);

        if previous.is_some() {
            self.invalidate_cache();
            self.add_chunk_viewable_packet(&graphite_mc_protocol::play::clientbound::BlockUpdate {
                pos: BlockPosition::new(x, y, z),
                block_state: block as _,
            })
        }
    }
}

slice_serializable! {
    pub struct DirectLevelChunkWithLight<'a> {
        pub chunk_x: i32 as BigEndian,
        pub chunk_z: i32 as BigEndian,
        pub chunk_block_data: &'a [u8] as WriteOnlyBlob,
        pub chunk_light_data: &'a [u8] as WriteOnlyBlob,
    }
}