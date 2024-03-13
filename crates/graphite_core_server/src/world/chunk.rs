use std::{borrow::Cow, rc::Rc, cell::UnsafeCell};

use graphite_binary::{slice_serialization::*, nbt::CachedNBT};
use graphite_mc_protocol::{play::{clientbound::{ChunkBlockData, ChunkLightData}, self}, IdentifiedPacket, types::BlockPosition};
use graphite_network::PacketBuffer;
use slab::Slab;

use crate::{entity::{EntityExtension, GenericEntity}, player::{GenericPlayer, PlayerExtension}, world::paletted_container::{BiomePalettedContainer, BlockPalettedContainer}};

use super::{chunk_section::ChunkSection, entity_iterator::{EntityIterator, EntityIteratorMut, PlayerIterator, PlayerIteratorMut}};

pub(crate) struct ChunkEntityRef(usize);
pub(crate) struct ChunkPlayerRef(usize);

pub struct Chunk {
    block_sections: Vec<ChunkSection>,

    pub(crate) entity_viewable: PacketBuffer,
    pub(crate) chunk_viewable: PacketBuffer,

    pub(crate) entities: Slab<Rc<UnsafeCell<dyn GenericEntity>>>,
    pub(crate) players: Slab<Rc<UnsafeCell<dyn GenericPlayer>>>,

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

        let mut all_sections_mask: u64 = 0;
        let mut block_light_mask: u64 = 0;
        let mut sky_light_mask: u64 = 0;
        let mut sky_light_entries = vec![];
        let mut block_light_entries = vec![];

        for i in 0..self.block_sections.len() {
            all_sections_mask |= 1 << (i+1);
            
            let block_section = &self.block_sections[i];
            if let Some(block_light) = &block_section.block_light {
                block_light_mask |= 1 << (i+1);
                block_light_entries.push(Cow::Borrowed(block_light.as_ref()));
            }
            if let Some(sky_light) = &block_section.sky_light {
                sky_light_mask |= 1 << (i+1);
                sky_light_entries.push(Cow::Borrowed(sky_light.as_ref()));
            }
        }
        all_sections_mask |= 1 | (1 << (self.block_sections.len()+1));

        let chunk_light_data = ChunkLightData {
            sky_light_mask: vec![sky_light_mask],
            block_light_mask: vec![block_light_mask],
            empty_sky_light_mask: vec![(!sky_light_mask) & all_sections_mask],
            empty_block_light_mask: vec![(!block_light_mask) & all_sections_mask],
            sky_light_entries,
            block_light_entries,
        };

        self.cached_block_data.clear();
        self.cached_block_data.write_raw(&chunk_block_data);

        self.cached_light_data.clear();
        self.cached_light_data.write_raw(&chunk_light_data);
    }

    pub fn players<P: PlayerExtension>(&self) -> PlayerIterator<'_, P> {
        PlayerIterator::new(self.players.iter(), false)
    }

    pub fn players_mut<P: PlayerExtension>(&mut self) -> PlayerIteratorMut<'_, P> {
        PlayerIteratorMut::new(self.players.iter_mut(), false)
    }

    pub fn entities<E: EntityExtension>(&mut self) -> EntityIterator<'_, E> {
        EntityIterator::new(self.entities.iter(), false)
    }

    pub fn entities_mut<E: EntityExtension>(&mut self) -> EntityIteratorMut<'_, E> {
        EntityIteratorMut::new(self.entities.iter_mut(), false)
    }

    pub(crate) fn insert_entity(&mut self, entity: Rc<UnsafeCell<dyn GenericEntity>>) -> ChunkEntityRef {
        let idx = self.entities.insert(entity);
        ChunkEntityRef(idx)
    }

    pub(crate) fn remove_entity(&mut self, chunk_ref: ChunkEntityRef) {
        self.entities.remove(chunk_ref.0);
    }

    pub(crate) fn insert_player(&mut self, player: Rc<UnsafeCell<dyn GenericPlayer>>) -> ChunkPlayerRef {
        let idx = self.players.insert(player);
        ChunkPlayerRef(idx)
    }

    pub(crate) fn remove_player(&mut self, chunk_ref: ChunkPlayerRef) {
        self.players.remove(chunk_ref.0);
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

    pub fn write_viewable(&mut self, mut lambda: impl FnMut(&mut PacketBuffer)) {
        lambda(&mut self.entity_viewable);
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

    pub fn has_players(&self) -> bool {
        !self.players.is_empty()
    }

    pub fn write_immediately_to_players(&mut self, data: &[u8]) {
        for (_, player) in &self.players {
            unsafe { player.get().as_mut().unwrap() }.send_packet_data(data);
        }
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
            players: Slab::new(),
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

    pub fn set_block_light_array(&mut self, section_y: usize, light: Box<[u8]>) {
        self.invalidate_cache();

        let section = &mut self.block_sections[section_y];

        section.block_light = Some(light);
    }

    pub fn set_block_light(&mut self, x: i32, y: i32, z: i32, mut light: u8) {
        self.invalidate_cache();

        let chunk_y = (y >> 4) as usize;
        let section = &mut self.block_sections[chunk_y];

        let index = ((((y & 0xF) << 8) | ((z & 0xF) << 4) | (x & 0xF)) / 2) as usize;

        if let Some(block_light) = &mut section.block_light {
            if x & 1 == 1 {
                block_light[index] &= 0x0F;
                block_light[index] |= (light << 4) & 0xF0;
            } else {
                block_light[index] &= 0xF0;
                block_light[index] |= light & 0xF;
            }
        } else {
            let mut block_light = vec![0_u8; 2048].into_boxed_slice();

            if x & 1 == 1 {
                light = (light << 4) & 0xF0;
            } else {
                light = light & 0xF;
            }

            block_light[index] = light;
            section.block_light = Some(block_light);
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