use std::{borrow::Cow, cell::UnsafeCell, collections::{hash_map::Entry, HashMap}, rc::Rc};

use graphite_binary::{slice_serialization::*, nbt::EncodedNBT};
use graphite_mc_protocol::{play::{self, clientbound::{BundledPacketBuffer, ChunkBlockData, ChunkLightData, RemoveEntities}}, types::BlockPosition, IdentifiedPacket};
use graphite_network::PacketBuffer;
use rustc_hash::FxHashMap;
use slab::Slab;

use crate::{entity::{entity_view::EntityView, remote_entity::RemoteEntity, EntityBase}, player::{GenericPlayer, Player, PlayerExtension}, types::AABB, world::paletted_container::{BiomePalettedContainer, BlockPalettedContainer}};

use super::{chunk_section::ChunkSection, entity_iterator::EntityIterator, player_iterator::{PlayerIterator, PlayerIteratorMut}, BlockGetter, PlayerId};

pub(crate) struct ChunkEntityRef {
    id: usize,
    entity_id: hecs::Entity
}
pub(crate) struct ChunkPlayerRef {
    id: usize,
    player_id: PlayerId
}

pub struct Chunk {
    block_sections: Vec<ChunkSection>,

    pub(crate) entity_viewable: PacketBuffer,
    pub(crate) chunk_viewable: PacketBuffer,
    pub(crate) single_block_changes: FxHashMap<u32, (u16, u16)>,

    pub(crate) players: Slab<Rc<UnsafeCell<dyn GenericPlayer>>>,
    pub(crate) entities: Slab<hecs::Entity>,

    pub(crate) solid_entity_aabbs: Vec<AABB>,
    pub(crate) pending_solid_entity_aabbs: Vec<AABB>,
    pub(crate) soft_entity_aabbs: Vec<AABB>,
    pub(crate) pending_soft_entity_aabbs: Vec<AABB>,

    valid_cache: bool,
    has_sent: bool,
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
            heightmaps: Cow::Borrowed(&[]),
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

    pub(crate) fn insert_entity(&mut self, entity: hecs::Entity) -> ChunkEntityRef {
        let idx = self.entities.insert(entity);

        ChunkEntityRef {
            id: idx,
            entity_id: entity
        }
    }

    pub(crate) fn remove_entity(&mut self, chunk_ref: ChunkEntityRef) {
        let removed = self.entities.remove(chunk_ref.id);
        if removed != chunk_ref.entity_id {
            panic!("Removed entity with wrong id");
        }
    }

    // pub fn test<Q: hecs::Query>(&self, query: &mut PreparedQuery<Q>, world: &hecs::World) {
    //     let mut borrow = query.query(world);
    //     let mut view = borrow.view();
    //     for (_, entity) in &self.entities {
    //         let res = view.get_mut(*entity);
    //     }
    // }

    pub fn entities<'a>(&'a self, world: &'a hecs::World) -> EntityIterator<'a> {
        EntityIterator::new(self.entities.iter(), world)
    }


    pub fn entity_ids(&self) -> slab::Iter<'_, hecs::Entity> {
        self.entities.iter()
    }

    pub(crate) fn insert_player(&mut self, player: Rc<UnsafeCell<dyn GenericPlayer>>) -> ChunkPlayerRef {
        let player_id = unsafe { player.get().as_ref() }.unwrap().get_player_id();
        let idx = self.players.insert(player);

        ChunkPlayerRef {
            id: idx,
            player_id
        }
    }

    pub(crate) fn remove_player(&mut self, chunk_ref: ChunkPlayerRef) {
        let removed = self.players.remove(chunk_ref.id);
        if unsafe { removed.get().as_ref() }.unwrap().get_player_id() != chunk_ref.player_id {
            panic!("Removed player with wrong id");
        }
    }

    pub(crate) fn clear_viewable_packets(&mut self) {
        self.entity_viewable.clear();
        self.chunk_viewable.clear();
        self.single_block_changes.clear();
    }

    pub fn add_entity_viewable_packet<'r, 'd: 'r, I: std::fmt::Debug, T>(&mut self, packet: &'r T)
    where
        T: SliceSerializable<'r, 'd, T> + IdentifiedPacket<I> + 'd,
    {
        packet.write_packet(&mut self.entity_viewable)
    }

    pub fn write_viewable(&mut self, lambda: impl FnOnce(&mut PacketBuffer)) {
        lambda(&mut self.entity_viewable);
    }

    pub fn add_chunk_viewable_packet<'r, 'd: 'r, I: std::fmt::Debug, T>(&mut self, packet: &'r T)
    where
        T: SliceSerializable<'r, 'd, T> + IdentifiedPacket<I> + 'd,
    {
        packet.write_packet(&mut self.chunk_viewable)
    }

    pub fn copy_chunk_viewable_packets(&self, buffer: &mut PacketBuffer) {
        buffer.copy_from(&self.chunk_viewable);
    }

    // todo: don't send to player when moving chunks
    pub fn write_spawn_entities_and_players<P: PlayerExtension>(
        &mut self,
        world: &hecs::World,
        remote_entities: &Slab<RemoteEntity>,
        player: &mut Player<P>
    ) {
        if !self.players.is_empty() {
            let player_id = player.entity_id;

            for (_, other) in &self.players {
                let other = unsafe { other.get().as_mut() }.unwrap();

                if other.get_entity_id() != player_id {
                    other.write_add_self_packet(&mut player.packet_buffer);
                    player.write_add_self_packet(other.get_packet_buffer());
                }
            }
        }

        let buffer = &mut player.packet_buffer;

        if !self.entities.is_empty() {
            let mut query = world.query::<(&EntityBase, Option<&EntityView>)>();
            let query_view = query.view();
    
            for (_, entity) in &mut self.entities {
                if let Some((base, view)) = query_view.get(*entity) {
                    let mut bundle = BundledPacketBuffer::new(buffer);

                    if let Some(view) = view {
                        if let Some(spawn) = view.spawn {
                            let entry = world.entity(*entity).unwrap();
                            (spawn)(entry, &*base, &*view, &mut *bundle);
                        }
                    }

                    for remote_entity in &base.remote_entities {
                        remote_entities[remote_entity.slab_index].spawn(&mut *bundle, base.position, base.rotation.y, base.rotation.x);
                    }
                }
            }
        }
    }

    pub fn write_despawn_entities_and_players<P: PlayerExtension>(
        &mut self,
        world: &hecs::World,
        remote_entities: &Slab<RemoteEntity>,
        despawn_vec: &mut Vec<i32>,
        player: &mut Player<P>
    ) {
        if !self.players.is_empty() {
            let player_id = player.entity_id;

            for (_, other) in &self.players {
                let other = unsafe { other.get().as_mut() }.unwrap();
                
                if other.get_entity_id() != player_id {
                    despawn_vec.push(other.get_entity_id());

                    RemoveEntities {
                        entities: Cow::Borrowed(&[player_id]),
                    }.write_packet(other.get_packet_buffer());
                }
            }
        }

        let buffer = &mut player.packet_buffer;

        if !self.entities.is_empty() {
            let mut query = world.query::<(&EntityBase, Option<&EntityView>)>();
            let query_view = query.view();
    
    
            for (_, entity) in &mut self.entities {
                if let Some((base, view)) = query_view.get(*entity) {
                    let mut bundle = BundledPacketBuffer::new(buffer);

                    for remote_entity in &base.remote_entities {
                        remote_entities[remote_entity.slab_index].spawn(&mut *bundle, base.position, base.rotation.y, base.rotation.x);
                    }

                    // Despawn all ids
                    if let Some(view) = view {
                        if !view.entity_ids.is_empty() {
                            despawn_vec.extend(&view.entity_ids);
                        }
                        
                        // Call custom despawn function
                        if let Some(despawn) = view.despawn {
                            let entry = world.entity(*entity).unwrap();
                            (despawn)(entry, &*base, &*view, despawn_vec, &mut *bundle);
                        }
                    }
                }
            }

        }
    }

    pub fn iter_entities(&self) -> slab::Iter<'_, hecs::Entity> {
        self.entities.iter()
    }

    pub fn has_players(&self) -> bool {
        !self.players.is_empty()
    }

    pub fn write_immediately_to_players(&mut self, data: &[u8]) {
        for (_, player) in &self.players {
            unsafe { player.get().as_mut().unwrap() }.get_packet_buffer().copy_bytes(data);
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
        self.has_sent = true;

        let composite = DirectLevelChunkWithLight {
            chunk_x,
            chunk_z,
            chunk_block_data: self.cached_block_data.peek_written(),
            chunk_light_data: self.cached_light_data.peek_written(),
        };

        let packet_id = play::clientbound::PlayPacket::LevelChunkWithLight as u8;
        let _ = packet_buffer.write_serializable(packet_id, &composite);
    }

    pub fn new(block_sections: Vec<ChunkSection>) -> Self {
        Self {
            block_sections,
            entity_viewable: PacketBuffer::new(),
            chunk_viewable: PacketBuffer::new(),
            single_block_changes: FxHashMap::default(),
            players: Slab::new(),
            entities: Slab::new(),
            solid_entity_aabbs: Vec::new(),
            pending_solid_entity_aabbs: Vec::new(),
            soft_entity_aabbs: Vec::new(),
            pending_soft_entity_aabbs: Vec::new(),
            valid_cache: false,
            has_sent: false,
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

    pub fn get_section(&self, chunk_y: i32) -> Option<&ChunkSection> {
        if chunk_y < 0 {
            return None;
        }

        let chunk_y = chunk_y as usize;
        if chunk_y >= self.block_sections.len() {
            return None; // out of bounds
        }

        let section = &self.block_sections[chunk_y];
        Some(section)
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

    pub(crate) fn get_block_for_client(&self, x: i32, y: i32, z: i32) -> Option<u16> {
        if y < 0 {
            return None;
        }

        let chunk_y = (y >> 4) as usize;
        if chunk_y >= self.block_sections.len() {
            return None; // out of bounds
        }

        let key = (((x & 0xF) as u32) << 28) | ((y as u32) << 4) | ((z & 0xF) as u32);
        if let Some((old_block, _)) = self.single_block_changes.get(&key) {
            return Some(*old_block);
        }

        let section = &self.block_sections[chunk_y];
        Some(section.get_block((x & 0xF) as _, (y & 0xF) as _, (z & 0xF) as _))
    }

    pub fn set_block(&mut self, x: i32, y: i32, z: i32, block: u16) -> u16 {
        let chunk_y = (y >> 4) as usize;
        let previous = self.block_sections[chunk_y].set_block((x & 0xF) as _, (y & 0xF) as _, (z & 0xF) as _, block);

        if self.has_sent && previous.is_some() {
            self.invalidate_cache();

            let key = (((x & 0xF) as u32) << 28) | ((y as u32) << 4) | ((z & 0xF) as u32);
            match self.single_block_changes.entry(key) {
                Entry::Occupied(mut occupied) => {
                    let old_previous = occupied.get().0;
                    occupied.insert((old_previous, block));
                },
                Entry::Vacant(vacant) => {
                    vacant.insert((previous.unwrap_or(block), block));
                },
            }
        }

        previous.unwrap_or(block)
    }

    pub fn set_block_light_array(&mut self, section_y: usize, light: Box<[u8]>) {
        self.invalidate_cache();
        let section = &mut self.block_sections[section_y];
        section.block_light = Some(light);
    }

    pub fn set_sky_light_array(&mut self, section_y: usize, light: Box<[u8]>) {
        self.invalidate_cache();
        let section = &mut self.block_sections[section_y];
        section.sky_light = Some(light);
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

    pub fn section_count(&self) -> usize {
        self.block_sections.len()
    }
}

pub trait ChunkProvider: BlockGetter {
    fn get_chunk(&self, x: i32, z: i32) -> Option<&Chunk>;
    fn get_chunk_mut(&mut self, x: i32, z: i32) -> Option<&mut Chunk>;

    fn set_block(&mut self, x: i32, y: i32, z: i32, block: u16) -> u16 {
        if let Some(chunk) = self.get_chunk_mut(x >> 4, z >> 4) {
            chunk.set_block(x, y, z, block)
        } else {
            0
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