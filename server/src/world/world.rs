use std::{collections::HashMap, time::Instant};

use anyhow::bail;
use bevy_ecs::{prelude::*, world::EntityRef};
use net::network_buffer::WriteBuffer;
use protocol::play::server::{
    Login, SetChunkCacheCenter, SetPlayerPosition, TeleportEntity, RotateHead,
};

use crate::{
    entity::{
        components::{Spinalla, BasicEntity, Viewable, EntitySpawnDefinition},
        position::{Position, Coordinate},
    },
    player::{proto_player::ProtoPlayer, Player, PlayerService},
    universe::{Universe, UniverseService},
};

use super::chunk::Chunk;

// user defined world service trait

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

    // # Safety
    // This method (WorldService::tick) should not be called directly
    // You should be calling World::tick, which will call this as well
    unsafe fn tick(world: &mut World<Self>);
}

// graphite world

pub struct World<W: WorldService + ?Sized> {
    universe: *mut Universe<W::UniverseServiceType>,
    pub(crate) chunks: Vec<Vec<Chunk>>,
    pub(crate) entities: bevy_ecs::world::World,
    empty_chunk: Chunk,
    pub service: W,
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
        let mut chunks = Vec::with_capacity(W::CHUNKS_X);
        for _ in 0..W::CHUNKS_X {
            let mut chunks_z = Vec::with_capacity(W::CHUNKS_Z);
            for _ in 0..W::CHUNKS_Z {
                chunks_z.push(Chunk::new(false));
            }
            chunks.push(chunks_z);
        }

        Self {
            service,
            universe: std::ptr::null_mut(),
            chunks,
            entities: Default::default(),
            empty_chunk: Chunk::new(true),
        }
    }

    pub fn initialize(&self, universe: &Universe<W::UniverseServiceType>) {
        // todo: justify this as being sound
        unsafe {
            let self_mut: *mut World<W> = self as *const _ as *mut _;
            let self_mut_ref: &mut World<W> = self_mut.as_mut().unwrap();
            assert!(self_mut_ref.universe.is_null(), "cannot initialize twice");
            self_mut_ref.universe = universe as *const _ as *mut _;
        }

        W::initialize(self);
    }

    pub fn push_entity<T: Bundle>(&mut self, components: T, position: Coordinate,
            mut spawn_def: impl EntitySpawnDefinition) {
        let fn_create = spawn_def.get_spawn_function();
        let destroy_buf = spawn_def.get_despawn_buffer();

        // Compute chunk coordinates
        let chunk_x = Chunk::to_chunk_coordinate(position.x);
        let chunk_z = Chunk::to_chunk_coordinate(position.z);

        // Debug checks that the chunk is in bounds
        debug_assert!(chunk_x >= 0, "position must be in-bounds");
        debug_assert!(chunk_z >= 0, "position must be in-bounds");
        debug_assert!(chunk_x < W::CHUNKS_X as _, "position must be in-bounds");
        debug_assert!(chunk_z < W::CHUNKS_Z as _, "position must be in-bounds");

        // Get the chunk
        let chunk = &mut self.chunks[chunk_x as usize][chunk_z as usize];
        
        // Spawn the entity in the bevy-ecs world
        let mut entity = self.entities.spawn();

        // Initialize viewable
        let mut viewable = Viewable::new(position, chunk_x, chunk_z, fn_create, destroy_buf);
        let id = entity.id();
        viewable.index_in_chunk_entity_slab = chunk.entities.insert(id);
        viewable.buffer = &mut chunk.viewable_buffer as *mut WriteBuffer;
        viewable.last_chunk_x = chunk_x;
        viewable.last_chunk_z = chunk_z;

        // Construct entity using components
        entity
            .insert_bundle(components)
            .insert(viewable);

        // Allow the spawn definition to add components
        spawn_def.add_components(&mut entity);
        
        // todo: why do we have to do this... can't we just convert EntityMut into EntityRef...
        // bevy.. please... im begging you
        // https://github.com/bevyengine/bevy/issues/5459
        let entity_ref = self.entities.entity(id);

        // Spawn the entity for players in the view distance of the chunk
        (fn_create)(&mut chunk.viewable_buffer, entity_ref);
    }

    pub fn tick(&mut self) {
        let start = Instant::now();

        // Update viewable buffer for entities

        // todo: this might have shit performance because we iterate over every entity
        // and then have to do a second map lookup, as opposed to just being able to iterate
        // over the EntityRefs. If this is actually how you're supposed to write this using
        // bevy-ecs I would be very surprised but the library is so incredibly obtuse that it
        // makes it impossible to figure out how to efficiently do things
        self.entities.query::<Entity>().for_each(&self.entities, |id| {
            let entity_ref = self.entities.entity(id);

            let mut viewable = unsafe { entity_ref.get_unchecked_mut::<Viewable>(0, 0) }
                .expect("all entities must have viewable");
    
            let chunk_x = Chunk::to_chunk_coordinate(viewable.coord.x);
            let chunk_z = Chunk::to_chunk_coordinate(viewable.coord.z);

            if viewable.last_chunk_x != chunk_x || viewable.last_chunk_z != chunk_z {
                let delta_x = chunk_x - viewable.last_chunk_x;
                let delta_z = chunk_z - viewable.last_chunk_z;
                let old_chunk_x = viewable.last_chunk_x;
                let old_chunk_z = viewable.last_chunk_z;
                viewable.last_chunk_x = chunk_x;
                viewable.last_chunk_z = chunk_z;

                let was_out_of_bounds = viewable.buffer.is_null();

                if chunk_x >= 0 && chunk_x < W::CHUNKS_X as _ {
                    if chunk_z >= 0 && chunk_z < W::CHUNKS_Z as _ {
                        // Remove from old entity list
                        if !was_out_of_bounds {
                            let old_chunk =
                                &mut self.chunks[old_chunk_x as usize][old_chunk_z as usize];
                            let id_in_list = old_chunk
                                .entities
                                .remove(viewable.index_in_chunk_entity_slab);
                            debug_assert_eq!(id_in_list, id);
                        }

                        // Update chunk entity list
                        let chunk = &mut self.chunks[chunk_x as usize][chunk_z as usize];
                        viewable.index_in_chunk_entity_slab = chunk.entities.insert(id);

                        // Update viewable entity's buffer ptr
                        viewable.buffer = &mut chunk.viewable_buffer as *mut WriteBuffer;

                        if was_out_of_bounds {
                            (viewable.fn_create)(&mut chunk.viewable_buffer, entity_ref);
                        } else {
                            // todo: maybe cache this write buffer?
                            let mut write_buffer = WriteBuffer::with_min_capacity(64);

                            (viewable.fn_create)(&mut write_buffer, entity_ref);
                            let create_bytes = write_buffer.get_written();
                            let destroy_bytes = viewable.destroy_buffer.get_written();

                            let self_chunks_ptrs = &mut self.chunks as *mut Vec<Vec<Chunk>>;

                            super::chunk_view_diff::for_each_diff_with_min_max(
                                (delta_x, delta_z),
                                W::ENTITY_VIEW_DISTANCE,
                                |x, z| {
                                    let chunk = &mut self.chunks[(old_chunk_x + x) as usize]
                                        [(old_chunk_z + z) as usize];
                                    chunk.copy_into_spot_buffer(create_bytes);
                                },
                                |x, z| {
                                    let chunks = unsafe { &mut *self_chunks_ptrs };
                                    let chunk = &mut chunks[(old_chunk_x + x) as usize]
                                        [(old_chunk_z + z) as usize];
                                    chunk.copy_into_spot_buffer(destroy_bytes);
                                },
                                -old_chunk_x,
                                -old_chunk_z,
                                W::CHUNKS_X as i32 - 1 - old_chunk_x,
                                W::CHUNKS_Z as i32 - 1 - old_chunk_z,
                            );
                        }

                        return;
                    }
                }

                // Entity entered out-of-bounds chunks
                if !was_out_of_bounds {
                    let buffer = unsafe { &mut *viewable.buffer };

                    // Send destroy packets to all viewers
                    buffer.copy_from(viewable.destroy_buffer.get_written());

                    // Remove from old entity list
                    let old_chunk = &mut self.chunks[old_chunk_x as usize][old_chunk_z as usize];
                    let id_in_list = old_chunk
                        .entities
                        .remove(viewable.index_in_chunk_entity_slab);
                    debug_assert_eq!(id_in_list, id);
                }
                viewable.buffer = std::ptr::null_mut();
            }
        });

        // Update entities
        // todo: call system::tick

        // todo: move to system
        self.entities.query::<(&mut Viewable, &mut Spinalla, &BasicEntity)>()
                .for_each_mut(&mut self.entities, |(mut viewable, mut spinalla, test_entity)| {
            if viewable.coord.x > 96.0 {
                spinalla.direction.0 = -spinalla.direction.0;
            } else if viewable.coord.x < 0.0 {
                spinalla.direction.0 = -spinalla.direction.0;
            }
            if viewable.coord.z > 96.0 {
                spinalla.direction.1 = -spinalla.direction.1;
            } else if viewable.coord.z < 0.0 {
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
        });

        // Tick service (ticks players)
        unsafe {
            W::tick(self);
        }

        // Clear viewable buffers
        for chunk_list in &mut self.chunks {
            for chunk in chunk_list {
                chunk.viewable_buffer.reset();
                chunk.viewable_buffer.tick_and_maybe_shrink();
                chunk.spot_buffer.reset();
                chunk.spot_buffer.tick_and_maybe_shrink();
            }
        }

        let stop = Instant::now();
        println!("Took: {:?}", stop.duration_since(start));
    }

    pub fn handle_player_join(&mut self, proto_player: ProtoPlayer<W::UniverseServiceType>) {
        W::handle_player_join(self, proto_player);
    }

    pub(crate) fn update_view_position<P: PlayerService>(
        &mut self,
        player: &mut Player<P>,
        position: Position,
    ) -> anyhow::Result<ChunkViewPosition> {
        // todo: send new chunks & entities

        let old_chunk_x = player.chunk_view_position.x as i32;
        let old_chunk_z = player.chunk_view_position.z as i32;
        let chunk_x = Chunk::to_chunk_coordinate(position.coord.x);
        let chunk_z = Chunk::to_chunk_coordinate(position.coord.z);

        let out_of_bounds = chunk_x < 0
            || chunk_x >= W::CHUNKS_X as _
            || chunk_z < 0
            || chunk_z >= W::CHUNKS_Z as _;
        let delta_x = chunk_x - old_chunk_x;
        let delta_z = chunk_z - old_chunk_z;
        let same_position = delta_x == 0 && delta_z == 0;
        if same_position || out_of_bounds {
            return Ok(player.chunk_view_position);
        }

        let chunk_view_position = ChunkViewPosition {
            x: chunk_x as usize,
            z: chunk_z as usize,
        };

        // Chunk
        // todo: only send new chunks
        let view_distance = W::CHUNK_VIEW_DISTANCE as i32;
        for x in -view_distance..view_distance + 1 {
            let chunk_x = x as i32 + chunk_view_position.x as i32;

            if chunk_x >= 0 && chunk_x < W::CHUNKS_X as _ {
                let chunk_list = &mut self.chunks[chunk_x as usize];

                for z in -view_distance..view_distance + 1 {
                    let chunk_z = z + chunk_view_position.z as i32;

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
                    let chunk_z = z + chunk_view_position.z as i32;
                    self.empty_chunk
                        .write(&mut player.write_buffer, chunk_x, chunk_z)?;
                }
            }
        }

        let mut temp_destroy_buffer = WriteBuffer::new(); // todo: don't use a temp buffer
        super::chunk_view_diff::for_each_diff_with_min_max(
            (delta_x, delta_z),
            W::ENTITY_VIEW_DISTANCE,
            |x, z| {
                let chunk = &self.chunks[(old_chunk_x + x) as usize][(old_chunk_z + z) as usize];
                chunk.entities.iter().for_each(|(_, id)| {
                    let entity = self.entities.entity(*id);

                    let viewable = entity.get::<Viewable>()
                        .expect("entity in chunk-list must be viewable");

                    (viewable.fn_create)(&mut player.write_buffer, entity);
                });
            },
            |x, z| {
                let chunk = &self.chunks[(old_chunk_x + x) as usize][(old_chunk_z + z) as usize];
                chunk.entities.iter().for_each(|(_, id)| {
                    let entity = self.entities.entity(*id);

                    let viewable = entity.get::<Viewable>()
                        .expect("entity in chunk-list must be viewable");

                    temp_destroy_buffer.copy_from(viewable.destroy_buffer.get_written());
                });
            },
            -old_chunk_x,
            -old_chunk_z,
            W::CHUNKS_X as i32 - 1 - old_chunk_x,
            W::CHUNKS_Z as i32 - 1 - old_chunk_z,
        );
        player
            .write_buffer
            .copy_from(temp_destroy_buffer.get_written());
        std::mem::drop(temp_destroy_buffer);

        // Create entities in (x, z)
        /*let chunk = &self.chunks[chunk_view_position.x][chunk_view_position.z];
        let mut query = <&Viewable>::query();
        chunk.entities.iter().for_each(|id| {
            if let Ok(viewable) = query.get(&self.entities, *id) {
                player.write_buffer.copy_from(viewable.create_buffer.get_written());
            }
        });*/

        // Update view position
        let update_view_position_packet = SetChunkCacheCenter {
            chunk_x: chunk_view_position.x as _,
            chunk_z: chunk_view_position.z as _,
        };
        player.write_packet(&update_view_position_packet);

        // Decrease the player count of the old chunk
        let old_chunk_out_of_bounds = old_chunk_x < 0
            || old_chunk_x >= W::CHUNKS_X as _
            || old_chunk_z < 0
            || old_chunk_z >= W::CHUNKS_Z as _;
        if !old_chunk_out_of_bounds {
            let old_chunk = &mut self.chunks[old_chunk_x as usize][old_chunk_z as usize];
            old_chunk.player_count -= 1;
        }

        // Increase the player count in the new chunk
        let spawning_chunk = &mut self.chunks[chunk_x as usize][chunk_z as usize];
        spawning_chunk.player_count += 1;

        Ok(chunk_view_position)
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

        // Chunk
        let view_distance = W::CHUNK_VIEW_DISTANCE as i32;
        for x in -view_distance..view_distance + 1 {
            let chunk_x = x + chunk_view_position.x as i32;

            if chunk_x >= 0 && chunk_x < W::CHUNKS_X as _ {
                let chunk_list = &mut self.chunks[chunk_x as usize];

                for z in -view_distance..view_distance + 1 {
                    let chunk_z = z + chunk_view_position.z as i32;

                    if chunk_z >= 0 && chunk_z < W::CHUNKS_Z as _ {
                        let chunk = &mut chunk_list[chunk_z as usize];
                        chunk.write(&mut proto_player.write_buffer, chunk_x, chunk_z)?;
                    } else {
                        self.empty_chunk
                            .write(&mut proto_player.write_buffer, chunk_x, chunk_z)?;
                    }
                }
            } else {
                for z in -view_distance..view_distance + 1 {
                    let chunk_z = z + chunk_view_position.z as i32;
                    self.empty_chunk
                        .write(&mut proto_player.write_buffer, chunk_x, chunk_z)?;
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
        let position_packet = SetPlayerPosition {
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

        // Increase the player count in the spawning chunk
        let spawning_chunk = &mut self.chunks[chunk_x as usize][chunk_z as usize];
        spawning_chunk.player_count += 1;

        Ok(chunk_view_position)
    }

    pub(crate) fn write_game_join_packet(
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
            gamemode: 1,
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
            has_death_location: false,
        };

        net::packet_helper::write_packet(&mut proto_player.write_buffer, &join_game_packet)?;

        net::packet_helper::write_packet(
            &mut proto_player.write_buffer,
            &self.get_universe().command_packet,
        )?;

        Ok(())
    }
}
