use std::collections::HashMap;

use anyhow::bail;
use bevy_ecs::{prelude::*, world::EntityMut};
use minecraft_constants::block::BlockProperties;
use net::network_buffer::WriteBuffer;
use protocol::play::server::{
    Login, PlayerPosition, RotateHead, SetChunkCacheCenter, TeleportEntity,
};

use crate::{
    entity::{
        components::{BasicEntity, EntitySpawnDefinition, Spinalla, Viewable},
        position::{Coordinate, Position},
    },
    player::{proto_player::ProtoPlayer, Player, PlayerService},
    universe::{EntityId, Universe, UniverseService},
};

use super::chunk::{BlockStorage, Chunk};

// user defined world service trait

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum TickPhaseInner {
    Update,
    View,
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct TickPhase(pub(crate) TickPhaseInner);

pub trait WorldService
where
    Self: Sized + 'static,
{
    type UniverseServiceType: UniverseService;
    const CHUNKS_X: usize;
    const CHUNKS_Z: usize;
    const CHUNK_VIEW_DISTANCE: u8 = 8;
    const ENTITY_VIEW_DISTANCE: u8 = 8;

    fn handle_player_join(
        world: &mut World<Self>,
        proto_player: ProtoPlayer<Self::UniverseServiceType>,
    );
    fn initialize(world: &World<Self>);
    fn get_player_count(world: &World<Self>) -> usize;

    fn tick(world: &mut World<Self>, phase: TickPhase);
}

// graphite world

pub struct World<W: WorldService + ?Sized> {
    universe: *mut Universe<W::UniverseServiceType>,

    pub service: W,
    pub(crate) entities: bevy_ecs::world::World,
    pub(crate) entity_map: HashMap<EntityId, bevy_ecs::entity::Entity>,

    // Don't move -- chunks must be dropped last
    pub(crate) chunks: Vec<Vec<Chunk>>, // todo: don't use Vec<Vec<>>
    empty_chunk: Chunk,
}

#[derive(Debug, Clone, Copy)]
pub struct ChunkViewPosition {
    pub(crate) x: usize, //todo: make private
    pub(crate) z: usize,
}

// graphite world impl

impl<W: WorldService> World<W> {
    pub fn get_universe(&mut self) -> &mut Universe<W::UniverseServiceType> {
        unsafe { self.universe.as_mut().unwrap() }
    }

    pub fn new(service: W) -> Self {
        // todo: these are default chunks, eventually this ctor should take
        // a list of chunks, or something equivalent
        let mut chunks = Vec::with_capacity(W::CHUNKS_X);
        for x in 0..W::CHUNKS_X {
            let mut chunks_z = Vec::with_capacity(W::CHUNKS_Z);
            for z in 0..W::CHUNKS_Z {
                chunks_z.push(Chunk::new(false, x, z));
            }
            chunks.push(chunks_z);
        }

        Self {
            service,
            universe: std::ptr::null_mut(),
            chunks,
            entities: Default::default(),
            entity_map: Default::default(),

            // todo: don't use this, send packets directly instead
            empty_chunk: Chunk::new(true, 0, 0),
        }
    }

    pub fn initialize(&self, universe: &Universe<W::UniverseServiceType>) {
        // Justification:
        // If the universe pointer is null, this struct is in an undefined state
        // Therefore, any reference that previously existed to this struct
        // is invalid, so converting the immutable reference to a mutable one
        // should be sound here
        unsafe {
            let self_mut: *mut World<W> = self as *const _ as *mut _;
            let self_mut_ref: &mut World<W> = self_mut.as_mut().unwrap();
            assert!(self_mut_ref.universe.is_null(), "cannot initialize twice");
            self_mut_ref.universe = universe as *const _ as *mut _;
        }

        W::initialize(self);
    }

    pub fn get_entity_mut(&mut self, entity_id: EntityId) -> Option<EntityMut> {
        if let Some(entity) = self.entity_map.get(&entity_id) {
            self.entities.get_entity_mut(*entity)
        } else {
            None
        }
    }

    pub fn push_entity<T: Bundle>(
        &mut self,
        components: T,
        position: Coordinate,
        mut spawn_def: impl EntitySpawnDefinition,
        entity_id: EntityId,
    ) {
        let fn_create = spawn_def.get_spawn_function();
        let destroy_buf = spawn_def.get_despawn_buffer();

        // Compute chunk coordinates
        let chunk_x = Chunk::to_chunk_coordinate(position.x);
        let chunk_z = Chunk::to_chunk_coordinate(position.z);

        // todo: return an error here instead of panicking,
        // invalid bounds can be caused by a player rather than implementation

        // Debug checks that the chunk is in bounds
        Self::assert_chunk_coords_in_bounds(chunk_x, chunk_z);

        // Get the chunk
        let chunk = &mut self.chunks[chunk_x as usize][chunk_z as usize];

        // Spawn the entity in the bevy-ecs world
        let mut entity = self.entities.spawn();

        // Map the Graphite EntityId to Bevy Entity
        let id = entity.id();
        self.entity_map.insert(entity_id, id);

        // Initialize viewable
        let mut viewable = Viewable::new(position, chunk_x, chunk_z, fn_create, destroy_buf);
        viewable.index_in_chunk_entity_slab = chunk.entities.insert(id);
        viewable.buffer = &mut chunk.entity_viewable_buffer as *mut WriteBuffer;
        viewable.last_chunk_x = chunk_x;
        viewable.last_chunk_z = chunk_z;

        // Construct entity using components
        entity.insert_bundle(components).insert(viewable);

        // Allow the spawn definition to add components
        spawn_def.add_components(&mut entity);

        // todo: why do we have to do this... can't we just convert EntityMut into EntityRef...
        // bevy.. please... im begging you
        // https://github.com/bevyengine/bevy/issues/5459
        let entity_ref = self.entities.entity(id);

        // Spawn the entity for players in the view distance of the chunk
        (fn_create)(&mut chunk.entity_viewable_buffer, entity_ref);
    }

    pub fn tick(&mut self) {
        // let start = Instant::now();

        // Update viewable state for entities
        self.update_viewable_entities();

        // Update entities
        // todo: call system::tick

        // todo: move to system
        self.entities
            .query::<(&mut Viewable, &mut Spinalla, &BasicEntity)>()
            .for_each_mut(
                &mut self.entities,
                |(mut viewable, mut spinalla, test_entity)| {
                    if viewable.coord.x > 100.0 || viewable.coord.x < -4.0 {
                        spinalla.direction.0 = -spinalla.direction.0;
                    }
                    if viewable.coord.z > 100.0 || viewable.coord.z < -4.0 {
                        spinalla.direction.1 = -spinalla.direction.1;
                    }

                    viewable.coord.x += spinalla.direction.0 * 0.5;
                    viewable.coord.z += spinalla.direction.1 * 0.5;
                    spinalla.rotation.yaw += 10.0;
                    spinalla.rotation.yaw %= 360.0;

                    let teleport = TeleportEntity {
                        entity_id: test_entity.entity_id.as_i32(),
                        x: viewable.coord.x as _,
                        y: viewable.coord.y as _,
                        z: viewable.coord.z as _,
                        yaw: spinalla.rotation.yaw,
                        pitch: spinalla.rotation.pitch,
                        on_ground: true,
                    };
                    viewable.write_viewable_packet(&teleport).unwrap();

                    let rotate_head = RotateHead {
                        entity_id: test_entity.entity_id.as_i32(),
                        head_yaw: spinalla.rotation.yaw,
                    };
                    viewable.write_viewable_packet(&rotate_head).unwrap();
                },
            );

        // Tick service (ticks players as well)
        W::tick(self, TickPhase(TickPhaseInner::Update));
        W::tick(self, TickPhase(TickPhaseInner::View));

        // Clear viewable buffers
        for chunk_list in &mut self.chunks {
            for chunk in chunk_list {
                chunk.entity_viewable_buffer.reset();
                chunk.entity_viewable_buffer.tick_and_maybe_shrink();
            }
        }

        // let end = Instant::now();
        // let took = end.duration_since(start);
        // println!("Took: {:?}", took);
    }

    fn update_viewable_entities(&mut self) {
        // todo: this might have shit performance because we iterate over every entity
        // and then have to do a second map lookup, as opposed to just being able to iterate
        // over the EntityRefs. If this is actually how you're supposed to write this using
        // bevy-ecs I would be very surprised but the library is so incredibly obtuse that it
        // makes it impossible to figure out how to efficiently do things
        self.entities
            .query::<Entity>()
            .for_each(&self.entities, |id| {
                let entity_ref = self.entities.entity(id);

                let mut viewable = unsafe { entity_ref.get_unchecked_mut::<Viewable>(0, 0) }
                    .expect("all entities must have viewable");

                let chunk_x = Chunk::to_chunk_coordinate(viewable.coord.x);
                let chunk_z = Chunk::to_chunk_coordinate(viewable.coord.z);

                if Self::chunk_coords_in_bounds(chunk_x, chunk_z) {
                    if viewable.last_chunk_x == chunk_x && viewable.last_chunk_z == chunk_z {
                        return;
                    }

                    // Remove from old entity list
                    let old_chunk = &mut self.chunks[viewable.last_chunk_x as usize]
                        [viewable.last_chunk_z as usize];
                    let id_in_list = old_chunk
                        .entities
                        .remove(viewable.index_in_chunk_entity_slab);
                    debug_assert_eq!(id_in_list, id);

                    // Update chunk entity list
                    let chunk = &mut self.chunks[chunk_x as usize][chunk_z as usize];
                    viewable.index_in_chunk_entity_slab = chunk.entities.insert(id);

                    // Update viewable entity's buffer ptr
                    viewable.buffer = &mut chunk.entity_viewable_buffer as *mut WriteBuffer;

                    // todo: maybe cache this write buffer?
                    let mut write_buffer = WriteBuffer::with_min_capacity(64);

                    (viewable.fn_create)(&mut write_buffer, entity_ref);
                    let create_bytes = write_buffer.get_written();
                    let destroy_bytes = viewable.destroy_buffer.get_written();

                    // Find chunk differences and write create/destroy packets
                    super::chunk_view_diff::for_each_diff_chunks(
                        (viewable.last_chunk_x, viewable.last_chunk_z),
                        (chunk_x, chunk_z),
                        W::ENTITY_VIEW_DISTANCE,
                        &mut self.chunks,
                        |chunk| {
                            chunk.write_to_players_in_chunk(create_bytes);
                        },
                        |chunk| {
                            chunk.write_to_players_in_chunk(destroy_bytes);
                        },
                        W::CHUNKS_X,
                        W::CHUNKS_Z,
                    );

                    viewable.last_chunk_x = chunk_x;
                    viewable.last_chunk_z = chunk_z;
                }
            });
    }

    pub fn handle_player_join(&mut self, proto_player: ProtoPlayer<W::UniverseServiceType>) {
        W::handle_player_join(self, proto_player);
    }

    /// # Safety
    /// This method must only be called by `Player::Drop`
    pub(crate) unsafe fn remove_player_from_chunk<P: PlayerService>(
        &mut self,
        player: &mut Player<P>,
    ) {
        let old_chunk =
            &mut self.chunks[player.chunk_view_position.x][player.chunk_view_position.z];
        old_chunk.destroy_player(player);
    }

    pub(crate) fn update_view_position<P: PlayerService>(
        &mut self,
        player: &mut Player<P>,
        position: Position,
    ) -> anyhow::Result<()> {
        let old_chunk_x = player.chunk_view_position.x as i32;
        let old_chunk_z = player.chunk_view_position.z as i32;
        let chunk_x = Chunk::to_chunk_coordinate(position.coord.x);
        let chunk_z = Chunk::to_chunk_coordinate(position.coord.z);

        let same_position = chunk_x == old_chunk_x && chunk_z == old_chunk_z;
        let out_of_bounds = !Self::chunk_coords_in_bounds(chunk_x, chunk_z);
        if same_position || out_of_bounds {
            return Ok(());
        }

        // Update view position
        let update_view_position_packet = SetChunkCacheCenter { chunk_x, chunk_z };
        player.write_packet(&update_view_position_packet);

        // Remove player from old internal chunk lists
        let player_ref = {
            let old_chunk = &mut self.chunks[old_chunk_x as usize][old_chunk_z as usize];
            old_chunk.pop_player_ref(player)
        };
        player.new_chunk_view_position = ChunkViewPosition {
            x: chunk_x as usize,
            z: chunk_z as usize,
        };

        // Chunk
        // todo: only send new chunks
        // holdup: currently using this behaviour for testing, to be able to see the server chunk state
        let view_distance = W::CHUNK_VIEW_DISTANCE as i32;
        for x in -view_distance..view_distance + 1 {
            let chunk_x = x as i32 + chunk_x;

            if chunk_x >= 0 && chunk_x < W::CHUNKS_X as _ {
                let chunk_list = &mut self.chunks[chunk_x as usize];

                for z in -view_distance..view_distance + 1 {
                    let chunk_z = z + chunk_z;

                    if chunk_z >= 0 && chunk_z < W::CHUNKS_Z as _ {
                        let chunk = &mut chunk_list[chunk_z as usize];
                        chunk.write(&mut player.write_buffer, chunk_x, chunk_z)?;
                    } else {
                        self.empty_chunk
                            .write(&mut player.write_buffer, chunk_x, chunk_z)?;
                    }
                }
            } else {
                for z in -view_distance..view_distance + 1 {
                    // todo: only need to send chunks 1 out, not all in render distance
                    let chunk_z = z + chunk_z;
                    self.empty_chunk
                        .write(&mut player.write_buffer, chunk_x, chunk_z)?;
                }
            }
        }

        // todo: maybe cache this write buffer?
        let mut create_buffer = WriteBuffer::with_min_capacity(64);
        player.write_create_packet(&mut create_buffer);
        let create_bytes = create_buffer.get_written();

        let mut destroy_buffer = WriteBuffer::with_min_capacity(64);
        player.write_destroy_packet(&mut destroy_buffer);
        let destroy_bytes = destroy_buffer.get_written();

        // Safety: closures just need to perform a single write call,
        // they don't rely on the previous state of the closure
        let player_write_buffer_ptr: *mut WriteBuffer = &mut player.write_buffer as *mut _;

        // Write create packets for now-visible entities and
        // destroy packets for no-longer-visible entities
        super::chunk_view_diff::for_each_diff_chunks(
            (old_chunk_x, old_chunk_z),
            (chunk_x, chunk_z),
            W::ENTITY_VIEW_DISTANCE,
            &mut self.chunks,
            |chunk| {
                // Get all entities in chunk
                chunk.entities.iter().for_each(|(_, id)| {
                    // Get viewable component
                    let entity = self.entities.entity(*id);
                    let viewable = entity
                        .get::<Viewable>()
                        .expect("entity in chunk-list must be viewable");

                    // Write create into player's buffer
                    (viewable.fn_create)(&mut player.write_buffer, entity);
                });

                // Create players
                chunk.write_create_for_players_in_chunk(&mut player.write_buffer);
                chunk.write_to_players_in_chunk(create_bytes);
            },
            |chunk| {
                // Access the write_buffer from the ptr
                let write_buffer = unsafe { &mut *player_write_buffer_ptr };

                // Get all entities in chunk
                chunk.entities.iter().for_each(|(_, id)| {
                    // Get viewable component
                    let entity = self.entities.entity(*id);
                    let viewable = entity
                        .get::<Viewable>()
                        .expect("entity in chunk-list must be viewable");

                    // Write destroy into player's buffer
                    write_buffer.copy_from(viewable.destroy_buffer.get_written());
                });

                // Destroy players
                chunk.write_destroy_for_players_in_chunk(write_buffer);
                chunk.write_to_players_in_chunk(destroy_bytes);
            },
            W::CHUNKS_X,
            W::CHUNKS_Z,
        );

        // Add player to new_chunk's internal player list
        let new_chunk = &mut self.chunks[chunk_x as usize][chunk_z as usize];
        new_chunk.push_player_ref(player, player_ref);

        Ok(())
    }

    pub(crate) fn initialize_view_position(
        &mut self,
        proto_player: &mut ProtoPlayer<W::UniverseServiceType>,
        position: Position,
    ) -> anyhow::Result<ChunkViewPosition> {
        let mut heightmap_nbt = quartz_nbt::NbtCompound::new();
        let mut motion_blocking_nbt = quartz_nbt::NbtList::new();
        for _ in 0..256 {
            motion_blocking_nbt.push(0_i64);
        }
        heightmap_nbt.insert("MOTION_BLOCKING", motion_blocking_nbt);

        let mut binary: Vec<u8> = Vec::new();
        quartz_nbt::io::write_nbt(
            &mut binary,
            None,
            &heightmap_nbt,
            quartz_nbt::io::Flavor::Uncompressed,
        )
        .unwrap();
        binary.shrink_to_fit();

        let chunk_x = Chunk::to_chunk_coordinate(position.coord.x);
        let chunk_z = Chunk::to_chunk_coordinate(position.coord.z);

        let out_of_bounds = chunk_x < 0
            || chunk_x >= W::CHUNKS_X as _
            || chunk_z < 0
            || chunk_z >= W::CHUNKS_Z as _;
        if out_of_bounds {
            bail!(
                "position (chunk_x: {}, chunk_z: {}) was out of bounds for this world",
                chunk_x,
                chunk_z
            );
        }

        let chunk_view_position = ChunkViewPosition {
            x: chunk_x as _,
            z: chunk_z as _,
        };

        // Chunk Data
        let view_distance = W::CHUNK_VIEW_DISTANCE as i32;
        for x in -view_distance..view_distance + 1 {
            // Calculate chunk_x and check if in bounds
            let chunk_x = x + chunk_view_position.x as i32;
            if chunk_x >= 0 && chunk_x < W::CHUNKS_X as _ {
                let chunk_list = &mut self.chunks[chunk_x as usize];

                // Calculate chunk_z and check if in bounds
                for z in -view_distance..view_distance + 1 {
                    let chunk_z = z + chunk_view_position.z as i32;
                    if chunk_z >= 0 && chunk_z < W::CHUNKS_Z as _ {
                        let chunk = &mut chunk_list[chunk_z as usize];

                        // Write actual chunk
                        chunk.write(&mut proto_player.write_buffer, chunk_x, chunk_z)?;
                    } else {
                        // Write dummy empty chunk
                        self.empty_chunk
                            .write(&mut proto_player.write_buffer, chunk_x, chunk_z)?;
                    }
                }
            } else {
                // Write dummy empty chunks
                for z in -view_distance..view_distance + 1 {
                    let chunk_z = z + chunk_view_position.z as i32;
                    self.empty_chunk
                        .write(&mut proto_player.write_buffer, chunk_x, chunk_z)?;
                }
            }
        }

        // Entities
        let view_distance = W::ENTITY_VIEW_DISTANCE as i32;
        for x in -view_distance..view_distance + 1 {
            // Calculate chunk_x and check if in bounds
            let chunk_x = x + chunk_view_position.x as i32;
            if chunk_x >= 0 && chunk_x < W::CHUNKS_X as _ {
                let chunk_list = &mut self.chunks[chunk_x as usize];

                // Calculate chunk_z and check if in bounds
                for z in -view_distance..view_distance + 1 {
                    let chunk_z = z + chunk_view_position.z as i32;
                    if chunk_z >= 0 && chunk_z < W::CHUNKS_Z as _ {
                        let chunk = &mut chunk_list[chunk_z as usize];

                        // Write entities
                        chunk.entities.iter().for_each(|(_, id)| {
                            // Get viewable component
                            let entity = self.entities.entity(*id);
                            let viewable = entity
                                .get::<Viewable>()
                                .expect("entity in chunk-list must be viewable");

                            // Use viewable to write create packet into player's buffer
                            (viewable.fn_create)(&mut proto_player.write_buffer, entity);
                        });

                        // Write players
                        chunk.write_create_for_players_in_chunk(&mut proto_player.write_buffer);
                    }
                }
            }
        }

        // Update view position
        let update_view_position_packet = SetChunkCacheCenter {
            chunk_x: chunk_view_position.x as _,
            chunk_z: chunk_view_position.z as _,
        };
        net::packet_helper::write_packet(
            &mut proto_player.write_buffer,
            &update_view_position_packet,
        )?;

        // Position
        let position_packet = PlayerPosition {
            x: position.coord.x as _,
            y: position.coord.y as _,
            z: position.coord.z as _,
            yaw: position.rot.yaw,
            pitch: position.rot.pitch,
            relative_arguments: 0,
            id: 0,
            dismount_vehicle: false,
        };
        net::packet_helper::write_packet(&mut proto_player.write_buffer, &position_packet)?;

        Ok(chunk_view_position)
    }

    pub(crate) fn write_login_packet(
        &mut self,
        //write_buffer: &mut WriteBuffer,
        proto_player: &mut ProtoPlayer<W::UniverseServiceType>,
    ) -> anyhow::Result<()> {
        let registry_codec =
            quartz_nbt::snbt::parse(include_str!("../../../assets/registry_codec.json")).unwrap();
        let mut binary: Vec<u8> = Vec::new();
        quartz_nbt::io::write_nbt(
            &mut binary,
            None,
            &registry_codec,
            quartz_nbt::io::Flavor::Uncompressed,
        )
        .unwrap();
        binary.shrink_to_fit();

        let join_game_packet = Login {
            entity_id: proto_player.entity_id.as_i32(),
            is_hardcore: proto_player.hardcore,
            gamemode: proto_player.abilities.gamemode as u8,
            previous_gamemode: -1,
            dimension_names: vec!["minecraft:overworld"],
            registry_codec: &binary,
            dimension_type: "minecraft:overworld",
            dimension_name: "minecraft:overworld",
            hashed_seed: 0, // affects biome noise
            max_players: 0, // unused
            view_distance: W::CHUNK_VIEW_DISTANCE as _,
            simulation_distance: W::ENTITY_VIEW_DISTANCE as _,
            reduced_debug_info: false,
            enable_respawn_screen: false,
            is_debug: false,
            is_flat: false,
            death_location: None,
        };

        net::packet_helper::write_packet(&mut proto_player.write_buffer, &join_game_packet)?;

        if let Some(command_packet) = &self.get_universe().command_packet {
            net::packet_helper::write_packet(&mut proto_player.write_buffer, command_packet)?;
        }

        Ok(())
    }

    pub fn get_required_destroy_ticks(&self, x: i32, y: i32, z: i32, speed: f32) -> Option<f32> {
        if let Some(block) = self.get_block_i32(x, y, z) {
            let properties: &BlockProperties = block.try_into().expect("valid block");

            if properties.air {
                None
            } else {
                Some(properties.hardness * speed)
            }
        } else {
            None
        }
    }

    pub fn get_destroy_stage(&self, x: i32, y: i32, z: i32, time: usize, speed: f32) -> Option<i8> {
        let destroy_ticks = self.get_required_destroy_ticks(x, y, z, speed)?;
        let break_progress = if destroy_ticks < 1.0 {
            1.0
        } else {
            time as f32 / destroy_ticks
        };

        let destroy_stage = if break_progress > 1.0 {
            8
        } else {
            (break_progress * 10.0 - 1.0).floor() as i8
        };

        Some(destroy_stage)
    }

    #[inline(always)]
    fn chunk_coords_in_bounds(chunk_x: i32, chunk_z: i32) -> bool {
        chunk_x >= 0 && chunk_x < W::CHUNKS_X as _ && chunk_z >= 0 && chunk_z < W::CHUNKS_Z as _
    }

    #[inline(always)]
    fn assert_chunk_coords_in_bounds(chunk_x: i32, chunk_z: i32) {
        debug_assert!(chunk_x >= 0, "position must be in-bounds");
        debug_assert!(chunk_z >= 0, "position must be in-bounds");
        debug_assert!(chunk_x < W::CHUNKS_X as _, "position must be in-bounds");
        debug_assert!(chunk_z < W::CHUNKS_Z as _, "position must be in-bounds");
    }
}

impl<W: WorldService + ?Sized> World<W> {
    pub fn set_block_i32(&mut self, x: i32, y: i32, z: i32, block: u16) -> Option<u16> {
        if x < 0 || y < 0 || z < 0 {
            return None;
        }
        self.set_block(x as _, y as _, z as _, block)
    }

    pub fn get_block_i32(&self, x: i32, y: i32, z: i32) -> Option<u16> {
        if x < 0 || y < 0 || z < 0 {
            return None;
        }
        self.get_block(x as _, y as _, z as _)
    }
}

impl<W: WorldService + ?Sized> BlockStorage for World<W> {
    fn fill_section_blocks(&mut self, y: usize, block: u16) {
        for chunk_list in &mut self.chunks {
            for chunk in chunk_list {
                chunk.fill_section_blocks(y, block);
            }
        }
    }

    fn set_block(&mut self, x: usize, y: usize, z: usize, block: u16) -> Option<u16> {
        let chunk_x = (x / Chunk::SECTION_BLOCK_WIDTH_I) as usize;
        let chunk_z = (z / Chunk::SECTION_BLOCK_WIDTH_I) as usize;

        if chunk_x < W::CHUNKS_X && chunk_z < W::CHUNKS_Z {
            let chunk = &mut self.chunks[chunk_x][chunk_z];
            chunk.set_block(x, y, z, block)
        } else {
            None
        }
    }

    fn get_block(&self, x: usize, y: usize, z: usize) -> Option<u16> {
        let chunk_x = (x / Chunk::SECTION_BLOCK_WIDTH_I) as usize;
        let chunk_z = (z / Chunk::SECTION_BLOCK_WIDTH_I) as usize;

        if chunk_x < W::CHUNKS_X && chunk_z < W::CHUNKS_Z {
            let chunk = &self.chunks[chunk_x][chunk_z];
            chunk.get_block(x, y, z)
        } else {
            None
        }
    }
}
