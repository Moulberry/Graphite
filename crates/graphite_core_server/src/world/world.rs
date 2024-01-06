use std::{rc::Rc, cell::UnsafeCell, ptr::NonNull};

use downcast_rs::Downcast;
use glam::DVec3;
use graphite_mc_protocol::play::{self, clientbound::{GameEventType, TagRegistry, Tag}};
use num::Zero;
use slab::Slab;

use crate::{ConfiguringPlayer, UniverseExtension, Universe, player::{GenericPlayer, PlayerExtension, Player}, entity::{GenericEntity, Entity, EntityExtension}, types::AABB};

use super::{chunk::Chunk, chunk_section::ChunkSection};

pub struct ChunkList {
    pub size_x: usize,
    pub size_y: usize,
    pub size_z: usize,
    pub chunks: Vec<Vec<ChunkSection>>,
}

pub trait GenericWorld: Downcast {
    fn tick(&mut self);
}
downcast_rs::impl_downcast!(GenericWorld);

impl <W: WorldExtension + 'static> GenericWorld for World<W> {
    fn tick(&mut self) {
        <World<W>>::tick(self);
    }
}

pub trait WorldExtension: Sized + 'static {
    type Universe: UniverseExtension;

    const CHUNKS_X: i32;
    const CHUNKS_Z: i32;
    const VIEW_DISTANCE: u8;
}

pub struct World<W: WorldExtension> {
    universe: NonNull<Universe<W::Universe>>,

    players: Slab<Rc<UnsafeCell<dyn GenericPlayer>>>,
    entities: Slab<Rc<UnsafeCell<dyn GenericEntity>>>,

    chunks: Box<[Chunk]>,
    pub(crate) empty_chunk: Chunk,

    collision_aabb_buffer: Vec<AABB>,

    extension: W
}

impl <W: WorldExtension + 'static> World<W> {
    pub fn new(universe: &mut Universe<W::Universe>, extension: W, chunk_list: ChunkList) -> Self {
        assert!(W::CHUNKS_X > 0);
        assert!(W::CHUNKS_Z > 0);
        assert!(W::VIEW_DISTANCE >= 2 && W::VIEW_DISTANCE <= 32);

        // todo: remove/add extra/empty chunks instead of panicing
        assert!(chunk_list.size_x == W::CHUNKS_X as usize);
        assert!(chunk_list.size_y == 24);
        assert!(chunk_list.size_z == W::CHUNKS_Z as usize);

        let mut chunks = Vec::with_capacity((W::CHUNKS_X * W::CHUNKS_Z) as usize);

        for index in 0..W::CHUNKS_X*W::CHUNKS_Z {
            // todo: maybe avoid clone?
            chunks.push(Chunk::new(chunk_list.chunks[index as usize].clone()));
        }

        Self {
            players: Slab::new(),
            entities: Slab::new(),

            chunks: chunks.into_boxed_slice(),
            empty_chunk: Chunk::new_empty(chunk_list.size_y),

            collision_aabb_buffer: Vec::new(),

            universe: universe.into(),
            extension
        }
    }

    pub fn extension(&mut self) -> &mut W {
        &mut self.extension
    }

    pub fn get_chunk(&self, x: i32, z: i32) -> Option<&Chunk> {
        if x < 0 || z < 0 || x >= W::CHUNKS_X || z >= W::CHUNKS_Z {
            None
        } else {
            Some(&self.chunks[(x + z * W::CHUNKS_X) as usize])
        }
    }

    pub fn get_chunk_mut(&mut self, x: i32, z: i32) -> Option<&mut Chunk> {
        if x < 0 || z < 0 || x >= W::CHUNKS_X || z >= W::CHUNKS_Z {
            None
        } else {
            Some(&mut self.chunks[(x + z * W::CHUNKS_X) as usize])
        }
    }

    pub fn spawn_new_entity<E: EntityExtension<World = W> + 'static>(&mut self, position: DVec3, extension: E) {
        let entity = Entity::new(self, position, extension);

        let chunk_x = entity.last_chunk_x;
        let chunk_z = entity.last_chunk_z;

        let entity = Rc::new(UnsafeCell::new(entity));

        self.entities.insert(entity.clone());

        if let Some(chunk) = self.get_chunk_mut(chunk_x, chunk_z) {
            let chunk_ref = chunk.insert_entity(entity.clone());

            let entity = unsafe { entity.get().as_mut() }.unwrap();
            entity.chunk_ref = Some(chunk_ref);
        }
    }

    pub fn spawn_new_player<P: PlayerExtension<World = W> + 'static>(&mut self, position: DVec3,
            player: ConfiguringPlayer<W::Universe>, extension: P) {
        let player = Player::new(self, position, player.connection.clone(), extension);
        let player_cell = Rc::new(UnsafeCell::new(player));
        self.players.insert(player_cell.clone());

        let player_ref = unsafe { player_cell.get().as_mut() }.unwrap();
        player_ref.connection.as_ref().unwrap().borrow_mut().set_handler(player_cell);

        // send join game
        let join_game = play::clientbound::JoinGame {
            entity_id: 0,
            is_hardcore: false,
            dimension_names: vec!["graphite:default_world"],
            max_players: 69420,
            view_distance: W::VIEW_DISTANCE as i32,
            simulation_distance: W::VIEW_DISTANCE as i32,
            reduced_debug_info: false,
            enable_respawn_screen: false,
            do_limited_crafting: false,
            dimension_type: "graphite:default_dimension_type",
            dimension_name: "graphite:default_world",
            hashed_seed: 0,
            gamemode: 1,
            previous_gamemode: -1,
            is_debug: false,
            is_flat: false,
            death_location: None,
            portal_cooldown: 0,
        };
        player_ref.write_packet(&join_game).unwrap();

        let tags = play::clientbound::UpdateTags {
            registries: vec![
                TagRegistry {
                    tag_type: "minecraft:fluid",
                    values: vec![
                        Tag {
                            name: "minecraft:water",
                            entries: vec![
                                1, 2
                            ]
                        },
                        Tag {
                            name: "minecraft:lava",
                            entries: vec![
                                3, 4
                            ]
                        }
                    ]
                }
            ],
        };
        player_ref.write_packet(&tags).unwrap();

        player_ref.write_packet(&play::clientbound::CustomPayload {
            channel: "minecraft:brand",
            data: b"\x08graphite",
        }).unwrap();

        // // send teleport
        player_ref.write_packet(&play::clientbound::PlayerPosition {
            x: position.x,
            y: position.y,
            z: position.z,
            yaw: 0.0,
            pitch: 0.0,
            relative_arguments: 0,
            id: 0
        }).unwrap();

        // send StartWaitingForLevelChunks game event
        player_ref.write_packet(&play::clientbound::GameEvent {
            event_type: GameEventType::StartWaitingForLevelChunks,
            param: 0.0,
        }).unwrap();

        let chunk_x = (position.x.floor() as i32) >> 4;
        let chunk_z = (position.z.floor() as i32) >> 4;

        player_ref.write_packet(&play::clientbound::SetChunkCacheCenter {
            chunk_x,
            chunk_z,
        }).unwrap();

        // write initial chunks
        let view_distance = W::VIEW_DISTANCE as i32 + 1;
        for x in (chunk_x-view_distance) .. (chunk_x+view_distance+1) {
            for z in (chunk_z-view_distance) .. (chunk_z+view_distance+1) {
                if let Some(chunk) = self.get_chunk_mut(x, z) {
                    // Write chunk information
                    chunk.write(&mut player_ref.packet_buffer, x, z);
                } else {
                    self.empty_chunk.write(&mut player_ref.packet_buffer, x, z);
                }
            }
        }

        // write initial entities
        let view_distance = W::VIEW_DISTANCE as i32;
        for x in (chunk_x-view_distance) .. (chunk_x+view_distance+1) {
            for z in (chunk_z-view_distance) .. (chunk_z+view_distance+1) {
                if let Some(chunk) = self.get_chunk_mut(x, z) {
                    // Write entity spawn packets
                    chunk.write_spawn_entities_and_players(&mut player_ref.packet_buffer);
                }
            }
        }

        // send all packets
        player_ref.flush_packets();
    }

    pub fn move_bounding_box_with_collision(&mut self, mut aabb: AABB, mut delta: DVec3) -> DVec3 {
        let Some(mut normalized) = delta.try_normalize() else {
            return DVec3::ZERO;
        };

        let expanded = aabb.expand(delta);
        let broad_phase_min = (expanded.min() - 1E-7).floor().as_ivec3() - 1;
        let broad_phase_max = (expanded.max() + 1E-7).floor().as_ivec3() + 1;

        let mut travelled = DVec3::ZERO;

        self.collision_aabb_buffer.clear();

        for x in broad_phase_min.x..broad_phase_max.x+1 {
            for y in broad_phase_min.y..broad_phase_max.y+1 {
                for z in broad_phase_min.z..broad_phase_max.z+1 {
                    if let Some(chunk) = self.get_chunk(x >> 4, z >> 4) {
                        if let Some(block) = chunk.get_block(x, y, z) {
                            if block != 0 {
                                let aabb = AABB::new(DVec3::new(x as f64, y as f64, z as f64),
                                    DVec3::new(x as f64 + 1.0, y as f64 + 1.0, z as f64 + 1.0)).unwrap();
                                self.collision_aabb_buffer.push(aabb);
                            }
                        }
                    }
                }
            }
        }

        if self.collision_aabb_buffer.is_empty() {
            return delta;
        }

        loop {
            let mut t = delta.length();
            let mut hit_face = usize::MAX;

            for block_aabb in &self.collision_aabb_buffer {                   
                let minkowski_difference = aabb.minkowski_difference(*block_aabb);
        
                if let Some((new_t, face)) = ray_box(-normalized, minkowski_difference) {
                    if new_t < t {
                        t = new_t;
                        hit_face = face;
                    }
                }
            }

            if hit_face == usize::MAX {
                self.collision_aabb_buffer.clear();
                return travelled + delta;
            } else {
                let to_collision = normalized * t;

                travelled += to_collision;
                delta -= to_collision;
                delta[hit_face] = 0.0;
                aabb = AABB::new(
                    aabb.min() + to_collision,
                    aabb.max() + to_collision
                ).unwrap();

                if let Some(new_normalized) = delta.try_normalize() {
                    normalized = new_normalized;
                } else {
                    self.collision_aabb_buffer.clear();
                    return travelled;
                }
            }

        }
    }

    fn tick(&mut self) {
        // Remove all players that have disconnected
        self.players.retain(|_, player| unsafe { player.get().as_mut() }.unwrap().is_valid());

        for (_, player) in &mut self.players {
            unsafe { player.get().as_mut() }.unwrap().tick();
        }
        for (_, entity) in &mut self.entities {
            unsafe { entity.get().as_mut() }.unwrap().tick();
        }
        for (_, player) in &mut self.players {
            unsafe { player.get().as_mut() }.unwrap().view_tick();
        }
        for chunk in self.chunks.iter_mut() {
            chunk.clear_viewable_packets();
        }
    }
}

fn ray_box(ray: DVec3, aabb: AABB) -> Option<(f64, usize)> {
    let mut t_near = f64::MIN;
    let mut t_far = f64::MAX;
    let mut face = 0;

    for i in 0..3 {
        if ray[i].is_zero() {
            if aabb.min()[i] >= 0.0 || aabb.max()[i] <= 0.0 {
                return None;
            }
        } else {
            let inverse = 1.0 / ray[i];

            let near;

            if inverse >= 0.0 {
                near = aabb.min()[i] * inverse;
                t_far = t_far.min(aabb.max()[i] * inverse);
            } else {
                near = aabb.max()[i] * inverse;
                t_far = t_far.min(aabb.min()[i] * inverse);
            }

            if near > t_near {
                t_near = near;
                face = i;
            }

            if t_near > t_far {
                return None;
            }
        }
    }

    if t_near >= 0.0 {
        Some((t_near, face))
    } else {
        None
    }
}
