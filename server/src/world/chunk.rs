use bevy_ecs::entity::Entity;
use binary::slice_serialization::{slice_serializable, BigEndian, GreedyBlob};
use net::{network_buffer::WriteBuffer, packet_helper};
use protocol::{play::server::{self, ChunkBlockData, ChunkLightData, BlockUpdate}, types::BlockPosition};
use slab::Slab;

use crate::player::{Player, PlayerService};

use super::{
    chunk_section::ChunkSection,
    paletted_container::{BiomePalettedContainer, BlockPalettedContainer},
};

pub struct PlayerReference {
    pub uuid: u128,
    pub player: *mut (),
    pub fn_write: fn(*mut (), &[u8]),
    pub fn_create: fn(*mut (), &mut WriteBuffer),
    pub destroy_buffer: Box<[u8]>,
}

pub struct Chunk {
    block_sections: Vec<ChunkSection>,

    chunk_x: usize,
    chunk_z: usize,

    valid_cache: bool,
    cached_block_data: WriteBuffer,
    cached_light_data: WriteBuffer,

    pub(crate) block_viewable_buffer: WriteBuffer,
    pub(crate) entity_viewable_buffer: WriteBuffer,
    pub(crate) entities: Slab<Entity>,
    player_refs: Slab<PlayerReference>,
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

    pub(crate) fn remove_player<T: PlayerService>(&mut self, player: &mut Player<T>) {
        let ref_index = player.chunk_ref;

        let removed = self
            .player_refs
            .try_remove(ref_index)
            .expect(Self::INVALID_NO_ENTRY);
        if removed.uuid != player.profile.uuid {
            panic!("{}", Self::INVALID_OTHER_PLAYER)
        }

        self.entity_viewable_buffer.copy_from(&removed.destroy_buffer);
    }

    pub(crate) fn move_player<T: PlayerService>(
        &mut self,
        player: &mut Player<T>,
        other_chunk: &mut Chunk,
    ) {
        let ref_index = player.chunk_ref;

        let removed = self
            .player_refs
            .try_remove(ref_index)
            .expect(Self::INVALID_NO_ENTRY);
        if removed.uuid != player.profile.uuid {
            panic!("{}", Self::INVALID_OTHER_PLAYER)
        }

        player.chunk_ref = other_chunk.player_refs.insert(removed);
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

    pub(crate) fn add_new_player<T: PlayerService>(&mut self, player: &mut Player<T>) {
        // Safety: write_create_packet doesn't touch `viewable_self_exclusion_write_buffer`
        let exclusion_write_buffer =
            unsafe { &mut *(&mut player.viewable_self_exclusion_write_buffer as *mut _) };
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

    pub fn new(empty: bool, chunk_x: usize, chunk_z: usize) -> Self {
        // Setup default block sections
        let mut block_sections = Vec::new();
        for i in 0..24 {
            if i < 18 && !empty {
                let chunk_section = ChunkSection::new(
                    16 * 16 * 16,
                    BlockPalettedContainer::filled(1),
                    BiomePalettedContainer::filled(1),
                );

                block_sections.push(chunk_section);
            } else {
                let chunk_section = ChunkSection::new(
                    0,
                    BlockPalettedContainer::filled(0),
                    BiomePalettedContainer::filled(1),
                );

                block_sections.push(chunk_section);
            }
        }

        Self {
            block_sections,
            valid_cache: false,
            cached_block_data: WriteBuffer::new(),
            cached_light_data: WriteBuffer::new(),
            chunk_x,
            chunk_z,
            block_viewable_buffer: WriteBuffer::new(),
            entity_viewable_buffer: WriteBuffer::new(),
            entities: Slab::new(),
            player_refs: Slab::new(),
        }
    }

    pub fn fill_blocks(&mut self, index: usize, block: u16) {
        if self.block_sections[index].fill_blocks(block) {
            self.invalidate_cache();
        }
    }

    pub fn set_block(&mut self, x: usize, y: usize, z: usize, block: u16) {
        let section_x = x % Self::SECTION_BLOCK_WIDTH_I;
        let section_y = y % Self::SECTION_BLOCK_WIDTH_I;
        let section_z = z % Self::SECTION_BLOCK_WIDTH_I;

        debug_assert_eq!(self.chunk_x, x / Self::SECTION_BLOCK_WIDTH_I, "set_block called on wrong chunk");
        let chunk_y = y / Self::SECTION_BLOCK_WIDTH_I + 4; // temp: remove + 4 when world limit is set to y = 0
        debug_assert_eq!(self.chunk_z, z / Self::SECTION_BLOCK_WIDTH_I, "set_block called on wrong chunk");

        if self.block_sections[chunk_y].set_block(section_x as _, section_y as _, section_z as _, block) {
            self.invalidate_cache();

            packet_helper::write_packet(&mut self.block_viewable_buffer, &BlockUpdate {
                pos: BlockPosition {
                    x: x as _,
                    y: y as _,
                    z: z as _,
                },
                block_state: block as _,
            }).expect("packet exceeds 2MB limit");
        }
    }

    fn invalidate_cache(&mut self) {
        // todo: maybe have more fine-grained invalidation here, not sure if its worth it
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

slice_serializable! {
    pub struct DirectLevelChunkWithLight<'a> {
        pub chunk_x: i32 as BigEndian,
        pub chunk_z: i32 as BigEndian,
        pub chunk_block_data: &'a [u8] as GreedyBlob,
        pub chunk_light_data: &'a [u8] as GreedyBlob,
    }
}
