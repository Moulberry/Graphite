use legion::*;
use legion::storage::IntoComponentSource;
use bytes::BufMut;
use net::network_buffer::WriteBuffer;
use protocol::{
    play::server::{
        ChunkBlockData, ChunkLightData, LevelChunkWithLight, Login, SetChunkCacheCenter,
        SetPlayerPosition, AddEntity, RemoveEntities, TeleportEntity, RotateHead,
    },
};

use crate::{
    player::{proto_player::ProtoPlayer, Player, PlayerService},
    universe::{Universe, UniverseService}, entity::{position::{Position, Coordinate}, components::{Viewable, TestEntity, Spinalla}}, world::chunk,
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
    const VIEW_DISTANCE: u8 = 8;

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
    pub(crate) entities: legion::World,
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

    pub fn push_entity<T>(&mut self, components: T)
    where
        Option<T>: IntoComponentSource,
    {
        self.entities.push(components);
    }

    pub fn tick(&mut self) {
        // Initialize entities
        // Option 1. Entities constantly write into the initialize buffer,
        //      if a player "discovers" the chunk, the bytes can be copied directly from there
        // *** Option 2. Entities DON'T constantly write into the initialize buffer,
        //      but when a player "discovers" a chunk, it has to query all entities in that chunk
        //      to get the spawn packet

        // Factor 1. the cost of constantly writing into the initialize buffer
        // Factor 2. the cost of querying all entities and getting their init packets
        // Factor 3. how often the players "discover" a chunk
        // Factor 1 > Factor 2 / Factor 3

        // Factor 1 == memcpy per entity
        // Factor 2 == iterating through every entity + memcpy per entity
        // Factor 3 == 1/200

        // Clear viewable buffers
        for chunk_list in &mut self.chunks {
            for chunk in chunk_list {
                chunk.viewable_buffer.reset();
                chunk.viewable_buffer.tick_and_maybe_shrink();
                chunk.clear_entity_refs();
            }
        }

        // Update viewable buffer for entities
        let mut query = <(Entity, &mut Viewable)>::query();
        query.for_each_mut(&mut self.entities, |(id, viewable)| {
            let chunk_x = Chunk::to_chunk_coordinate(viewable.coord.x);
            let chunk_z = Chunk::to_chunk_coordinate(viewable.coord.z);

            viewable.buffer = if chunk_x >= 0 && chunk_x < W::CHUNKS_X as _ {
                if chunk_z >= 0 && chunk_z < W::CHUNKS_Z as _ {
                    let chunk = &mut self.chunks[chunk_x as usize][chunk_z as usize];
                    chunk.entities.push(*id);
                    &mut chunk.viewable_buffer as *mut WriteBuffer
                } else {
                    std::ptr::null_mut()
                }
            } else {
                std::ptr::null_mut()
            };
        });

        // Update entities
        let mut query = <(&mut Viewable, &mut TestEntity)>::query();
        for (viewable, test_entity) in query.iter_mut(&mut self.entities) {
            if !test_entity.spawned {
                let add_entity_packet = AddEntity {
                    id: 87123,
                    uuid: 128371283,
                    entity_type: test_entity.entity_type,
                    x: viewable.coord.x as _,
                    y: viewable.coord.y as _,
                    z: viewable.coord.z as _,
                    yaw: 0.0,
                    pitch: 0.0,
                    head_yaw: 0.0,
                    data: 0,
                    x_vel: 0.0,
                    y_vel: 0.0,
                    z_vel: 0.0,
                };
                if viewable.write_viewable_packet(&add_entity_packet).unwrap() {
                    viewable.write_create_packet(&add_entity_packet).unwrap();

                    let remove_packet = RemoveEntities {
                        entities: vec![87123]
                    };
                    viewable.write_destroy_packet(&remove_packet).unwrap();

                    test_entity.spawned = true;
                }
            }
        }

        let mut query = <(&mut Viewable, &mut Spinalla)>::query();
        for (viewable, spinalla) in query.iter_mut(&mut self.entities) {
            spinalla.rotation.yaw += 1.0;
            spinalla.rotation.yaw %= 360.0;
            let teleport = TeleportEntity {
                entity_id: 87123,
                x: viewable.coord.x as _,
                y: viewable.coord.y as _,
                z: viewable.coord.z as _,
                yaw: spinalla.rotation.yaw,
                pitch: spinalla.rotation.pitch,
                on_ground: true,
            };
            viewable.write_viewable_packet(&teleport).unwrap();
            let rotate_head = RotateHead {
                entity_id: 87123,
                head_yaw: spinalla.rotation.yaw
            };
            viewable.write_viewable_packet(&rotate_head).unwrap();
        }

        // Tick service (ticks players)
        unsafe { W::tick(self); }
    }

    pub fn handle_player_join(&mut self, proto_player: ProtoPlayer<W::UniverseServiceType>) {
        W::handle_player_join(self, proto_player);
    }

    pub(crate) fn update_view_position<P: PlayerService>(
        &self,
        player: &mut Player<P>,
        position: Position,
    ) -> anyhow::Result<ChunkViewPosition> {
        // todo: send new chunks & entities

        let old_chunk_x = player.chunk_view_position.x as i32;
        let old_chunk_z = player.chunk_view_position.z as i32;
        let chunk_x = Chunk::to_chunk_coordinate(position.coord.x);
        let chunk_z = Chunk::to_chunk_coordinate(position.coord.z);

        let out_of_bounds = chunk_x < 0 || chunk_x >= W::CHUNKS_X as _ ||
                                 chunk_z < 0 || chunk_z >= W::CHUNKS_Z as _;
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
        for x in -10..10 {
            let chunk_x = x + chunk_view_position.x as i32;

            if chunk_x >= 0 && chunk_x < W::CHUNKS_X as _ {
                let chunk_list = &self.chunks[chunk_x as usize];

                for z in -10..10 {
                    let chunk_z = z + chunk_view_position.z as i32;

                    if chunk_z >= 0 && chunk_z < W::CHUNKS_Z as _ {
                        let chunk = &chunk_list[chunk_z as usize];
                        chunk.write(&mut player.write_buffer, chunk_x, chunk_z)?;
                    } else {
                        self.empty_chunk
                            .write(&mut player.write_buffer, chunk_x, chunk_z)?;
                    }
                }
            } else {
                for z in -10..10 {
                    let chunk_z = z + chunk_view_position.z as i32;
                    self.empty_chunk
                        .write(&mut player.write_buffer, chunk_x, chunk_z)?;
                }
            }
        }

        let mut temp_destroy_buffer = WriteBuffer::new();
        super::chunk_view_diff::for_each_diff_with_min_max((delta_x, delta_z), W::VIEW_DISTANCE, |x, z| {
            let chunk = &self.chunks[(old_chunk_x + x) as usize][(old_chunk_z + z) as usize];
            let mut query = <&Viewable>::query();
            chunk.entities.iter().for_each(|id| {
                if let Ok(viewable) = query.get(&self.entities, *id) {
                    player.write_buffer.copy_from(viewable.create_buffer.get_written());
                }
            });
        }, |x, z| {
            println!("unloading chunk: {},{}", old_chunk_x + x, old_chunk_z + z);
            let chunk = &self.chunks[(old_chunk_x + x) as usize][(old_chunk_z + z) as usize];
            let mut query = <&Viewable>::query();
            chunk.entities.iter().for_each(|id| {
                if let Ok(viewable) = query.get(&self.entities, *id) {
                    temp_destroy_buffer.copy_from(viewable.destroy_buffer.get_written());
                }
            });
        }, -old_chunk_x, -old_chunk_z, W::CHUNKS_X as i32 - 1 - old_chunk_x, W::CHUNKS_Z as i32 - 1 - old_chunk_z);

        player.write_buffer.copy_from(temp_destroy_buffer.get_written());
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

        // Chunk
        for x in -10..10 {
            for z in -10..10 {
                let mut chunk_data = vec![0_u8; 0];
                for i in 0..24 {
                    chunk_data.put_i16(16 * 16 * 16); // block count

                    // blocks
                    chunk_data.put_u8(0); // single pallete, 0 bits per entry
                    if i < 18 && x + z != 0 {
                        chunk_data.put_u8(1); // palette. stone
                    } else {
                        chunk_data.put_u8(0); // palette. air
                    }
                    chunk_data.put_u8(0); // 0 size array

                    // biomes
                    chunk_data.put_u8(0); // single pallete, 0 bits per entry
                    chunk_data.put_u8(1); // some biome
                    chunk_data.put_u8(0); // 0 size array
                }

                let chunk_packet = LevelChunkWithLight {
                    chunk_x: x,
                    chunk_z: z,
                    chunk_block_data: ChunkBlockData {
                        heightmaps: &binary,
                        data: &chunk_data,
                        block_entity_count: 0,
                        trust_edges: true,
                    },
                    chunk_light_data: ChunkLightData {
                        sky_light_mask: vec![],
                        block_light_mask: vec![],
                        empty_sky_light_mask: vec![],
                        empty_block_light_mask: vec![],
                        sky_light_entries: vec![],
                        block_light_entries: vec![],
                    },
                };
                net::packet_helper::write_packet(&mut proto_player.write_buffer, &chunk_packet)?;
            }
        }

        let chunk_view_position = ChunkViewPosition {
            x: Chunk::to_chunk_coordinate(position.coord.x) as _,
            z: Chunk::to_chunk_coordinate(position.coord.z) as _,
        };

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

        println!("codec bytes: {}", binary.len());

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
            view_distance: W::VIEW_DISTANCE as _,
            simulation_distance: W::VIEW_DISTANCE as _,
            reduced_debug_info: false,
            enable_respawn_screen: false,
            is_debug: false,
            is_flat: false,
            has_death_location: false,
        };

        net::packet_helper::write_packet(&mut proto_player.write_buffer, &join_game_packet)?;

        net::packet_helper::write_packet(&mut proto_player.write_buffer, &self.get_universe().command_packet)?;

        Ok(())
    }
}
