use std::borrow::Cow;

use bevy_ecs::entity::Entity;
use graphite_binary::{
    nbt::{CachedNBT},
    slice_serialization::{slice_serializable, BigEndian, GreedyBlob},
};

use graphite_net::{network_buffer::WriteBuffer, packet_helper};
use graphite_mc_protocol::{
    play::server::{self, BlockUpdate, ChunkBlockData, ChunkLightData},
    types::BlockPosition,
};
use slab::Slab;

use crate::player::{Player, PlayerService};

use super::{
    chunk_section::ChunkSection,
    paletted_container::{BiomePalettedContainer, BlockPalettedContainer}, block_entity_storage::BlockEntityStorage,
};
pub(crate) struct PlayerReference {
    uuid: u128,
    player: *mut (),
    fn_write: fn(*mut (), &[u8]),
    fn_create: fn(*mut (), &mut WriteBuffer),
    destroy_buffer: Box<[u8]>,
}

pub struct Chunk {
    block_sections: Vec<ChunkSection>,
    pub(crate) block_entities: BlockEntityStorage,

    valid_cache: bool,
    cached_block_data: WriteBuffer,
    cached_light_data: WriteBuffer,

    pub(crate) block_viewable_buffer: WriteBuffer,
    pub(crate) entity_viewable_buffer: WriteBuffer,
    pub(crate) entities: Slab<Entity>,
    player_refs: Slab<PlayerReference>,
}

impl Clone for Chunk {
    fn clone(&self) -> Self {
        Self {
            block_sections: self.block_sections.clone(),
            block_entities: self.block_entities.clone(),
            valid_cache: false,
            cached_block_data: WriteBuffer::with_min_capacity(0),
            cached_light_data: WriteBuffer::with_min_capacity(0),
            block_viewable_buffer: WriteBuffer::with_min_capacity(0),
            entity_viewable_buffer: WriteBuffer::with_min_capacity(0),
            entities: Slab::new(),
            player_refs: Slab::new(),
        }
    }
}

impl Chunk {
    pub const SECTION_BLOCK_WIDTH_F: f32 = 16.0;
    pub const SECTION_BLOCK_WIDTH_I: usize = 16;

    const INVALID_NO_ENTRY: &'static str = "player's chunk_ref is invalid - no entry for reference";
    const INVALID_OTHER_PLAYER: &'static str =
        "player's chunk_ref is invalid - entry was for another player";

    #[inline(always)]
    pub fn to_chunk_coordinate(f: f32) -> i32 {
        (f / Chunk::SECTION_BLOCK_WIDTH_F).floor() as i32
    }

    pub fn get_block_sections(&self) -> &[ChunkSection] {
        self.block_sections.as_slice()
    }

    pub(crate) fn expand(&mut self, increase_y: isize) {
        if increase_y == 0 {
            return;
        }

        self.invalidate_cache();

        let abs_increase_y = increase_y.abs() as usize;
        self.block_sections.reserve_exact(abs_increase_y);

        let empty = ChunkSection::new(
            0,
            BlockPalettedContainer::filled(0),
            BiomePalettedContainer::filled(0),
        );        

        if increase_y > 0 {
            for _ in 0..increase_y {
                self.block_sections.push(empty.clone());
            }
        } else {
            self.block_sections.splice(0..0, std::iter::repeat(empty).take(abs_increase_y));
        }
    }

    pub(crate) fn write_to_players_in_chunk(&mut self, bytes: &[u8]) {
        for (_, reference) in &self.player_refs {
            (reference.fn_write)(reference.player, bytes);
        }
    }

    pub(crate) fn write_create_for_players_in_chunk(&self, write_buffer: &mut WriteBuffer) {
        for (_, reference) in &self.player_refs {
            (reference.fn_create)(reference.player, write_buffer);
        }
    }

    pub(crate) fn write_destroy_for_players_in_chunk(&self, write_buffer: &mut WriteBuffer) {
        for (_, reference) in &self.player_refs {
            write_buffer.copy_from(&reference.destroy_buffer);
        }
    }

    pub(crate) fn destroy_player<T: PlayerService>(&mut self, player: &Player<T>) {
        let ref_index = player.chunk_ref;

        let removed = self
            .player_refs
            .try_remove(ref_index)
            .expect(Self::INVALID_NO_ENTRY);
        if removed.uuid != player.profile.uuid {
            panic!("{}", Self::INVALID_OTHER_PLAYER)
        }

        self.entity_viewable_buffer
            .copy_from(&removed.destroy_buffer);
    }

    pub(crate) fn pop_all_entities(&mut self) -> Slab<Entity> {
        std::mem::replace(&mut self.entities, Slab::new())
    }

    pub(crate) fn push_all_entities(&mut self, entities: Slab<Entity>) {
        assert!(self.entities.is_empty());
        let _ = std::mem::replace(&mut self.entities, entities);
    }

    pub(crate) fn pop_all_player_refs(&mut self) -> Slab<PlayerReference> {
        std::mem::replace(&mut self.player_refs, Slab::new())
    }

    pub(crate) fn push_all_player_refs(&mut self, refs: Slab<PlayerReference>) {
        assert!(self.player_refs.is_empty());
        let _ = std::mem::replace(&mut self.player_refs, refs);
    }

    pub(crate) fn pop_player_ref<T: PlayerService>(
        &mut self,
        player: &mut Player<T>,
    ) -> PlayerReference {
        let reference = self
            .player_refs
            .try_remove(player.chunk_ref)
            .expect(Self::INVALID_NO_ENTRY);
        if reference.uuid != player.profile.uuid {
            panic!("{}", Self::INVALID_OTHER_PLAYER)
        }

        player.chunk_ref = usize::MAX;
        reference
    }

    pub(crate) fn push_player_ref<T: PlayerService>(
        &mut self,
        player: &mut Player<T>,
        reference: PlayerReference,
    ) {
        player.chunk_ref = self.player_refs.insert(reference);
    }

    pub(crate) fn update_player_pointer<T: PlayerService>(&mut self, player: &mut Player<T>) {
        let ref_index = player.chunk_ref;

        let reference = self
            .player_refs
            .get_mut(ref_index)
            .expect(Self::INVALID_NO_ENTRY);
        if reference.uuid != player.profile.uuid {
            panic!("{}", Self::INVALID_OTHER_PLAYER)
        }

        reference.player = player as *mut _ as *mut ();
    }

    pub(crate) fn create_player<T: PlayerService>(&mut self, player: &mut Player<T>) {
        // Safety: write_create_packet doesn't touch `viewable_self_exclusion_write_buffer`
        let exclusion_write_buffer =
            unsafe { &mut *(&mut player.packets.viewable_self_exclusion_write_buffer as *mut _) };
        player.write_create_packet(exclusion_write_buffer);

        // Get ptr to write_packet_bytes function
        let write_packet_bytes = Player::<T>::write_packet_bytes as *const ();
        let fn_write: fn(*mut (), &[u8]) = unsafe { std::mem::transmute(write_packet_bytes) };

        // Get ptr to write_create_packet function
        let write_create_packet = Player::<T>::write_create_packet as *const ();
        let fn_create: fn(*mut (), &mut WriteBuffer) =
            unsafe { std::mem::transmute(write_create_packet) };

        let mut destroy_buffer = WriteBuffer::new();
        player.write_destroy_packet(&mut destroy_buffer);

        let reference = PlayerReference {
            uuid: player.profile.uuid,
            player: player as *mut _ as *mut _,
            fn_write,
            fn_create,
            destroy_buffer: destroy_buffer.get_written().into(),
        };

        player.chunk_ref = self.player_refs.insert(reference);
    }

    pub fn new(block_sections: Vec<ChunkSection>) -> Self {
        Self {
            block_sections,
            block_entities: BlockEntityStorage::new(),
            valid_cache: false,
            cached_block_data: WriteBuffer::with_min_capacity(0),
            cached_light_data: WriteBuffer::with_min_capacity(0),
            block_viewable_buffer: WriteBuffer::with_min_capacity(0),
            entity_viewable_buffer: WriteBuffer::with_min_capacity(0),
            entities: Slab::new(),
            player_refs: Slab::new(),
        }
    }

    pub fn new_empty(size_y: usize) -> Self {
        // Setup default block sections
        let mut block_sections = Vec::with_capacity(size_y);

        for _ in 0..size_y {
            block_sections.push(ChunkSection::new(
                0,
                BlockPalettedContainer::filled(0),
                BiomePalettedContainer::filled(0),
            ));
        }

        assert_eq!(block_sections.capacity(), size_y);
        assert_eq!(block_sections.len(), size_y);
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

    fn invalidate_cache(&mut self) {
        // todo: maybe have more fine-grained invalidation here, not sure if its worth it
        self.valid_cache = false;
    }

    fn compute_cache(&mut self) {
        self.valid_cache = true;

        // Write chunk data
        let mut chunk_data = WriteBuffer::new();
        for block_section in &mut self.block_sections {
            packet_helper::write_slice_serializable(&mut chunk_data, block_section);
        }
        let chunk_block_data = ChunkBlockData {
            heightmaps: Cow::Owned(CachedNBT::new()),
            data: chunk_data.get_written(),
            block_entity_count: self.block_entities.count() as i32,
            block_entity_data: self.block_entities.bytes(),
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

        self.cached_block_data.clear();
        graphite_net::packet_helper::write_slice_serializable(
            &mut self.cached_block_data,
            &chunk_block_data,
        );

        self.cached_light_data.clear();
        graphite_net::packet_helper::write_slice_serializable(
            &mut self.cached_light_data,
            &chunk_light_data,
        );
    }

    pub fn write_into_self(&mut self, chunk_x: i32, chunk_z: i32) -> anyhow::Result<()> {
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
        graphite_net::packet_helper::write_custom_packet(
            &mut self.block_viewable_buffer,
            packet_id, 
            &composite
        )
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
        graphite_net::packet_helper::write_custom_packet(write_buffer, packet_id, &composite)
    }
}

pub trait BlockStorage {
    fn fill_section_blocks(&mut self, y: usize, block: u16);
    fn set_block(&mut self, x: usize, y: usize, z: usize, block: u16) -> Option<u16>;
    fn get_block(&self, x: usize, y: usize, z: usize) -> Option<u16>;
}

impl BlockStorage for Chunk {
    fn fill_section_blocks(&mut self, y: usize, block: u16) {
        if y >= self.block_sections.len() {
            return; // out of bounds
        }

        if self.block_sections[y].fill_blocks(block) {
            self.invalidate_cache();
        }
    }

    fn get_block(&self, x: usize, y: usize, z: usize) -> Option<u16> {
        let section_x = x % Self::SECTION_BLOCK_WIDTH_I;
        let section_y = y % Self::SECTION_BLOCK_WIDTH_I;
        let section_z = z % Self::SECTION_BLOCK_WIDTH_I;

        let chunk_y = y / Self::SECTION_BLOCK_WIDTH_I;
        if chunk_y >= self.block_sections.len() {
            return None; // out of bounds
        }

        let section = &self.block_sections[chunk_y];
        Some(section.get_block(section_x as _, section_y as _, section_z as _))
    }

    fn set_block(&mut self, x: usize, y: usize, z: usize, block: u16) -> Option<u16> {
        let section_x = x % Self::SECTION_BLOCK_WIDTH_I;
        let section_y = y % Self::SECTION_BLOCK_WIDTH_I;
        let section_z = z % Self::SECTION_BLOCK_WIDTH_I;

        let chunk_y = y / Self::SECTION_BLOCK_WIDTH_I;
        if chunk_y >= self.block_sections.len() {
            return None; // out of bounds
        }

        let section = &mut self.block_sections[chunk_y];
        if let Some(old) = section.set_block(section_x as _, section_y as _, section_z as _, block)
        {
            /*let the_block: &Block = block.try_into().unwrap();
            match the_block {
                Block::SkeletonSkull { rotation } => {
                    section.block_entities.get_or_create_mut(section_x as _, section_y as _, section_z as _, 15);
                }
                Block::OakSign { rotation, waterlogged } => {
                    let block_entity = section.block_entities.get_or_create_mut(section_x as _,
                            section_y as _, section_z as _, 7);
                    block_entity.nbt.insert_root("Text1", NBTNode::String("{\"text\": \"Hello World!\"}".into()))
                }
                _ => ()
            }*/

            self.invalidate_cache();

            packet_helper::try_write_packet(
                &mut self.block_viewable_buffer,
                &BlockUpdate {
                    pos: BlockPosition {
                        x: x as _,
                        y: y as _,
                        z: z as _,
                    },
                    block_state: block as _,
                },
            );

            Some(old)
        } else {
            None
        }
    }
}

slice_serializable! {
    pub struct DirectLevelChunkWithLight<'a> {
        pub chunk_x: i32 as BigEndian,
        pub chunk_z: i32 as BigEndian,
        pub chunk_block_data: &'a [u8] as GreedyBlob,
        pub chunk_light_data: &'a [u8] as GreedyBlob,
    }
}
