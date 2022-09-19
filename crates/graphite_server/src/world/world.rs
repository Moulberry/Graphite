use std::collections::HashMap;

use bevy_ecs::{prelude::*, world::EntityMut};
use graphite_mc_constants::{block::BlockAttributes, item::Item};
use graphite_net::network_buffer::WriteBuffer;
use graphite_mc_protocol::{
    play::server::{PlayerPosition, RotateHead, SetChunkCacheCenter, TeleportEntity, InitializeBorder, ForgetLevelChunk},
    types::{BlockPosition, Direction},
};
use graphite_sticky::Unsticky;

use crate::{
    entity::{
        components::{BasicEntity, EntitySpawnDefinition, Spinalla, Viewable},
        position::{Coordinate, Position, Rotation},
    },
    player::{proto_player::ProtoPlayer, Player, PlayerService},
    universe::{EntityId, Universe, UniverseService}, ticker::WorldTicker,
};

use super::{
    chunk::{BlockStorage, Chunk},
    placement_context::ServerPlacementContext, chunk_list::ChunkGrid,
};

// user defined world service trait

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum TickPhaseInner {
    Update,
    View,
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct TickPhase(pub(crate) TickPhaseInner);

pub trait WorldService: WorldTicker<Self>
where
    Self: Sized + 'static,
{
    type UniverseServiceType: UniverseService;
    type ParentWorldServiceType: WorldService;

    const CHUNK_VIEW_DISTANCE: u8 = 8;
    const ENTITY_VIEW_DISTANCE: u8 = 8;
    const SHOW_DEFAULT_WORLD_BORDER: bool = false;

    fn handle_player_join(
        world: &mut World<Self>,
        proto_player: ProtoPlayer<Self::UniverseServiceType>,
    );
}

// graphite world

pub struct World<W: WorldService + ?Sized> {
    universe: *mut Universe<W::UniverseServiceType>,
    parent_world: *mut World<W::ParentWorldServiceType>,

    pub service: W,
    pub(crate) entities: bevy_ecs::world::World,
    pub(crate) entity_map: HashMap<EntityId, bevy_ecs::entity::Entity>,
    pub(crate) global_write_buffer: WriteBuffer,

    // Don't move -- chunks must be dropped last
    pub(crate) chunks: ChunkGrid,
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

    pub fn new(service: W, chunks: ChunkGrid) -> Self {
        Self {
            universe: std::ptr::null_mut(),
            parent_world: std::ptr::null_mut(),

            service,
            entities: Default::default(),
            entity_map: Default::default(),
            global_write_buffer: Default::default(),

            empty_chunk: Chunk::new_empty(chunks.size_y()),
            chunks,
        }
    }

    pub fn new_with_empty_chunks(service: W, size_x: usize, size_y: usize, size_z: usize) -> Self {
        Self::new(service, ChunkGrid::new_with_empty_chunks(size_x, size_y, size_z))
    }

    pub fn new_with_default_chunks(service: W, size_x: usize, size_y: usize, size_z: usize) -> Self {
        Self::new(service, ChunkGrid::new_with_default_chunks(size_x, size_y, size_z))
    }

    pub fn update_universe_ptr(&mut self, universe: *mut Universe<W::UniverseServiceType>) {
        self.universe = universe;
        self.service.update_universe_ptr(universe);
    }

    pub fn update_parent_world_ptr(&mut self, world: *mut World<W::ParentWorldServiceType>) {
        self.parent_world = world;
    }

    pub fn get_entity_mut(&mut self, entity_id: EntityId) -> Option<EntityMut> {
        if let Some(entity) = self.entity_map.get(&entity_id) {
            self.entities.get_entity_mut(*entity)
        } else {
            None
        }
    }

    pub fn expand(&mut self, increase_x: isize, increase_y: isize, increase_z: isize) {
        self.empty_chunk.expand(increase_y);
        self.chunks.expand(increase_x, increase_y, increase_z);

        if increase_x < 0 || increase_y < 0 || increase_z < 0 {
            for (x, z, chunk) in self.chunks.enumerate_mut() {
                chunk.write_into_self(x as _, z as _).unwrap();
            }
        }

        // Remove World Border
        if W::SHOW_DEFAULT_WORLD_BORDER {
            let border_packet = InitializeBorder {
                x: 0.0,
                z: 0.0,
                old_diameter: 29999984.0,
                new_diameter: 29999984.0,
                speed: 0,
                portal_teleport_boundary: 29999984,
                warning_blocks: 0,
                warning_time: 0
            };
            graphite_net::packet_helper::try_write_packet(&mut self.global_write_buffer, &border_packet);
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

        // Compute chunk coordinates, clamped to valid coordinates
        let chunk_x = Chunk::to_chunk_coordinate(position.x).max(0).min(self.chunks.size_x() as i32 - 1);
        let chunk_z = Chunk::to_chunk_coordinate(position.z).max(0).min(self.chunks.size_z() as i32 - 1);

        // Get the chunk
        let chunk = self.chunks.get_mut(chunk_x as usize, chunk_z as usize).unwrap();

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
                    viewable.write_viewable_packet(&teleport);

                    let rotate_head = RotateHead {
                        entity_id: test_entity.entity_id.as_i32(),
                        head_yaw: spinalla.rotation.yaw,
                    };
                    viewable.write_viewable_packet(&rotate_head);
                },
            );

        // Tick service (ticks players as well)
        self.service.tick(TickPhase(TickPhaseInner::Update));
        self.service.tick(TickPhase(TickPhaseInner::View));

        // Clear viewable buffers
        for chunk in self.chunks.iter_mut() {
            chunk.entity_viewable_buffer.clear();
            chunk.block_viewable_buffer.clear();
        }
        self.global_write_buffer.clear();

        // let end = Instant::now();
        // let took = end.duration_since(start);
        // println!("Took: {:?}", took);
    }

    fn update_viewable_entities(&mut self) {
        let size_x = self.chunks.size_x();
        let size_z = self.chunks.size_z();

        let mut temp_write_buffer = WriteBuffer::with_min_capacity(64);

        // todo: this might have shit performance because we iterate over every entity
        // and then have to do a second map lookup, as opposed to just being able to iterate
        // over the EntityRefs. If this is actually how you're supposed to write this using
        // bevy-ecs I would be very surprised but the library is so incredibly obtuse that it
        // makes it impossible to figure out how to efficiently do things
        self.entities
            .query::<Entity>().for_each(&self.entities, |id| {
                let entity_ref = self.entities.entity(id);

                let mut viewable = unsafe { entity_ref.get_unchecked_mut::<Viewable>(0, 0) }
                    .expect("all entities must have viewable");

                let chunk_x = Chunk::to_chunk_coordinate(viewable.coord.x);
                let chunk_z = Chunk::to_chunk_coordinate(viewable.coord.z);

                if let Some(chunk) = self.chunks.get_mut(chunk_x as usize, chunk_z as usize) {
                    if viewable.last_chunk_x == chunk_x && viewable.last_chunk_z == chunk_z {
                        return;
                    }

                    // Update chunk entity list
                    viewable.index_in_chunk_entity_slab = chunk.entities.insert(id);

                    // Update viewable entity's buffer ptr
                    viewable.buffer = &mut chunk.entity_viewable_buffer as *mut WriteBuffer;

                    // Remove from old entity list
                    if let Some(old_chunk) = self.chunks.get_mut_i32(viewable.last_chunk_x, viewable.last_chunk_z) {
                        let id_in_list = old_chunk
                            .entities.remove(viewable.index_in_chunk_entity_slab);
                        debug_assert_eq!(id_in_list, id);
                    }

                    temp_write_buffer.clear();
                    (viewable.fn_create)(&mut temp_write_buffer, entity_ref);
                    let create_bytes = temp_write_buffer.get_written();
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
                        size_x,
                        size_z
                    );

                    viewable.last_chunk_x = chunk_x;
                    viewable.last_chunk_z = chunk_z;
                }
            });
    }

    pub fn handle_player_join(&mut self, proto_player: ProtoPlayer<W::UniverseServiceType>) {
        W::handle_player_join(self, proto_player);
    }

    pub(crate) fn update_view_position<P: PlayerService>(
        &mut self,
        player: &mut Player<P>,
        position: Position,
    ) -> anyhow::Result<()> {
        let old_chunk_x = player.new_chunk_view_position.x as i32;
        let old_chunk_z = player.new_chunk_view_position.z as i32;
        let chunk_x = Chunk::to_chunk_coordinate(position.coord.x);
        let chunk_z = Chunk::to_chunk_coordinate(position.coord.z);

        let same_position = chunk_x == old_chunk_x && chunk_z == old_chunk_z;
        let out_of_bounds = !self.chunk_coords_in_bounds(chunk_x, chunk_z);
        if same_position || out_of_bounds {
            return Ok(());
        }

        // Chunk
        // todo: only send new chunks
        // holdup: currently using this behaviour for testing, to be able to see the server chunk state
        let view_distance = W::CHUNK_VIEW_DISTANCE as i32;
        for x in -view_distance..view_distance + 1 {
            let chunk_x = x as i32 + chunk_x;
            for z in -view_distance..view_distance + 1 {
                let chunk_z = z + chunk_z;

                if let Some(chunk) = self.chunks.get_mut_i32(chunk_x, chunk_z) {
                    chunk.write(&mut player.packets.write_buffer, chunk_x, chunk_z)?;
                } else {
                    self.empty_chunk.write(
                        &mut player.packets.write_buffer,
                        chunk_x,
                        chunk_z,
                    )?;
                }
            }
        }

        // Update view position
        let update_view_position_packet = SetChunkCacheCenter { chunk_x, chunk_z };
        player.packets.write_packet(&update_view_position_packet);

        // Remove player from old internal chunk lists
        let player_ref = {
            let old_chunk = self.chunks.get_mut(old_chunk_x as usize, old_chunk_z as usize)
                .expect("chunk coords in bounds");
            old_chunk.pop_player_ref(player)
        };
        player.new_chunk_view_position = ChunkViewPosition {
            x: chunk_x as usize,
            z: chunk_z as usize,
        };

        // todo: maybe cache this write buffer?
        let mut create_buffer = WriteBuffer::with_min_capacity(64);
        player.write_create_packet(&mut create_buffer);
        let create_bytes = create_buffer.get_written();

        let mut destroy_buffer = WriteBuffer::with_min_capacity(64);
        player.write_destroy_packet(&mut destroy_buffer);
        let destroy_bytes = destroy_buffer.get_written();

        // Safety: closures just need to perform a single write call,
        // they don't rely on the previous state of the closure
        let player_write_buffer_ptr: *mut WriteBuffer = &mut player.packets.write_buffer as *mut _;

        // Write create packets for now-visible entities and
        // destroy packets for no-longer-visible entities
        let size_x = self.chunks.size_x();
        let size_z = self.chunks.size_z();
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
                    (viewable.fn_create)(&mut player.packets.write_buffer, entity);
                });

                // Create players
                chunk.write_create_for_players_in_chunk(&mut player.packets.write_buffer);
                chunk.write_to_players_in_chunk(create_bytes);

                // todo: send chunk packet here
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

                // todo: send chunk packet here
            },
            size_x,
            size_z,
        );

        // Add player to new_chunk's internal player list
        let new_chunk = self.chunks.get_mut_i32(chunk_x, chunk_z).expect("chunk coords in bounds");
        new_chunk.push_player_ref(player, player_ref);

        // World Border
        if W::SHOW_DEFAULT_WORLD_BORDER {
            let border_packet = self.make_default_world_border(chunk_z, chunk_x);
            player.packets.write_packet(&border_packet);
        }

        Ok(())
    }

    pub(crate) fn remove_player<P: PlayerService>(&mut self, player: &mut Player<P>, view_position: ChunkViewPosition) {
        if let Some(old_chunk) = self.chunks.get_mut(view_position.x, view_position.z) {
            old_chunk.destroy_player(player);
        }

        let view_distance = W::CHUNK_VIEW_DISTANCE as i32;
        for x in -view_distance..view_distance + 1 {
            let chunk_x = x + view_position.x as i32;
            for z in -view_distance..view_distance + 1 {
                let chunk_z = z + view_position.z as i32;

                let unload = ForgetLevelChunk { chunk_x, chunk_z };
                player.packets.write_packet(&unload);
            }
        }
    }

    pub(crate) fn initialize_view_position(
        &mut self,
        proto_player: &mut ProtoPlayer<W::UniverseServiceType>,
        position: Position,
    ) -> ChunkViewPosition {
        let chunk_x = Chunk::to_chunk_coordinate(position.coord.x).max(0).min(self.chunks.size_x() as i32 - 1);
        let chunk_z = Chunk::to_chunk_coordinate(position.coord.z).max(0).min(self.chunks.size_z() as i32 - 1);

        let chunk_view_position = ChunkViewPosition {
            x: chunk_x as _,
            z: chunk_z as _,
        };

        // Chunk Data
        let view_distance = W::CHUNK_VIEW_DISTANCE as i32;
        for x in -view_distance..view_distance + 1 {
            let chunk_x = x + chunk_view_position.x as i32;
            for z in -view_distance..view_distance + 1 {
                let chunk_z = z + chunk_view_position.z as i32;

                if let Some(chunk) = self.chunks.get_mut_i32(chunk_x, chunk_z) {
                    chunk.write(&mut proto_player.write_buffer, chunk_x, chunk_z).unwrap();
                } else {
                    self.empty_chunk
                        .write(&mut proto_player.write_buffer, chunk_x, chunk_z).unwrap();
                }
            }
        }

        // Update view position
        let update_view_position_packet = SetChunkCacheCenter {
            chunk_x: chunk_view_position.x as _,
            chunk_z: chunk_view_position.z as _,
        };
        graphite_net::packet_helper::try_write_packet(
            &mut proto_player.write_buffer,
            &update_view_position_packet,
        );

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
        graphite_net::packet_helper::try_write_packet(&mut proto_player.write_buffer, &position_packet);

        // Entities
        let view_distance = W::ENTITY_VIEW_DISTANCE as i32;
        for x in -view_distance..view_distance + 1 {
            let chunk_x = x + chunk_view_position.x as i32;
            for z in -view_distance..view_distance + 1 {
                let chunk_z = z + chunk_view_position.z as i32;

                if let Some(chunk) = self.chunks.get_mut_i32(chunk_x, chunk_z) {
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

        // World Border
        if W::SHOW_DEFAULT_WORLD_BORDER {
            let border_packet = self.make_default_world_border(chunk_z, chunk_x);
            graphite_net::packet_helper::try_write_packet(&mut proto_player.write_buffer, &border_packet);
        }

        chunk_view_position
    }

    fn make_default_world_border(&mut self, chunk_z: i32, chunk_x: i32) -> InitializeBorder {
        let size_x = self.chunks.size_x();
        let size_z = self.chunks.size_z();

        let mut x = (size_x * 8) as f64;
        let mut z = (size_z * 8) as f64;
        let mut diameter = (size_x * 16) as f64;

        if size_x < size_z {
            diameter = (size_z * 16) as f64;
            if chunk_x <= (size_x as i32)/2 {
                x = z;
            } else {
                x = x * 2.0 - z;
            }
        } else if size_z < size_x {
            if chunk_z <= (size_z as i32)/2 {
                z = x;
            } else {
                z = z * 2.0 - x;
            }
        }

        InitializeBorder {
            x,
            z,
            old_diameter: diameter,
            new_diameter: diameter,
            speed: 0,
            portal_teleport_boundary: 29999984,
            warning_blocks: 0,
            warning_time: 0
        }
    }

    pub fn create_placement_context(
        &mut self,
        pos: BlockPosition,
        face: Direction,
        click_offset: (f32, f32, f32),
        placer_rot: Rotation,
        placed_item: Item,
    ) -> Option<(ServerPlacementContext<W>, BlockPosition)> {
        let block = self.get_block_i32(pos.x, pos.y, pos.z)?;
        if let Ok(attributes) = <&BlockAttributes>::try_from(block) {
            let (offset_pos, existing_block_id) = if attributes.replaceable {
                (pos, block)
            } else {
                let offset_pos = pos.relative(face);
                let offset_block = self.get_block_i32(offset_pos.x, offset_pos.y, offset_pos.z)?;
                if let Ok(offset_attributes) = <&BlockAttributes>::try_from(offset_block) {
                    if offset_attributes.replaceable {
                        (offset_pos, offset_block)
                    } else {
                        return None;
                    }
                } else {
                    return None;
                }
            };

            let ctx = ServerPlacementContext {
                interacted_pos: pos,
                offset_pos,
                click_offset,
                face,
                placer_yaw: placer_rot.yaw,
                placer_pitch: placer_rot.pitch,
                placed_item,
                world: self,
                existing_block_id: Some(Some(existing_block_id)),
                existing_block: None
            };
    
            Some((ctx, offset_pos))
        } else {
            None
        }

    }

    pub fn get_required_destroy_ticks(&self, x: i32, y: i32, z: i32, speed: f32) -> Option<f32> {
        if let Some(block) = self.get_block_i32(x, y, z) {
            let properties: &BlockAttributes = block.try_into().expect("valid block");

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
    fn chunk_coords_in_bounds(&self, chunk_x: i32, chunk_z: i32) -> bool {
        chunk_x >= 0 && chunk_x < self.chunks.size_x() as _ && chunk_z >= 0 && chunk_z < self.chunks.size_z() as _
    }

    pub fn get_chunks(&self) -> &ChunkGrid {
        &self.chunks
    }

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

impl<W: WorldService + ?Sized> Unsticky for World<W> {
    type UnstuckType = Self;

    fn update_pointer(&mut self) {
        let self_ptr = self as *mut _;
        self.service.update_children_ptr(self_ptr);
    }

    fn unstick(self) -> Self::UnstuckType {
        self
    }
}

impl<W: WorldService + ?Sized> BlockStorage for World<W> {
    fn fill_section_blocks(&mut self, y: usize, block: u16) {
        for chunk in self.chunks.iter_mut() {
            chunk.fill_section_blocks(y, block);
        }
    }

    fn set_block(&mut self, x: usize, y: usize, z: usize, block: u16) -> Option<u16> {
        let chunk_x = (x / Chunk::SECTION_BLOCK_WIDTH_I) as usize;
        let chunk_z = (z / Chunk::SECTION_BLOCK_WIDTH_I) as usize;

        self.chunks.get_mut(chunk_x, chunk_z)
            .and_then(|chunk| chunk.set_block(x, y, z, block))
    }

    fn get_block(&self, x: usize, y: usize, z: usize) -> Option<u16> {
        let chunk_x = (x / Chunk::SECTION_BLOCK_WIDTH_I) as usize;
        let chunk_z = (z / Chunk::SECTION_BLOCK_WIDTH_I) as usize;

        self.chunks.get(chunk_x, chunk_z)
            .and_then(|chunk| chunk.get_block(x, y, z))
    }
}
