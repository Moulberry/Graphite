use std::{borrow::Cow, cell::{RefCell, UnsafeCell}, collections::HashMap, hash::BuildHasherDefault, marker::PhantomData, ops::{AddAssign, BitOr, BitOrAssign}, ptr::NonNull, rc::Rc, time::Instant};

use downcast_rs::Downcast;
use glam::DVec3;
use graphite_mc_constants::{block::{Block, BlockAttributes}, item::Item};
use graphite_mc_protocol::play::{self, clientbound::{GameEventType, LevelParticles, Tag, TagRegistry}};
use num::Zero;
use rustc_hash::FxHasher;
use slab::Slab;

use crate::{entity::{Entity, EntityExtension, GenericEntity}, particle::Particle, player::{GenericPlayer, Player, PlayerExtension}, types::AABB, ConfiguringPlayer, Universe, UniverseExtension};

use super::{chunk::{Chunk, ChunkEntityRef, ChunkPlayerRef}, chunk_section::ChunkSection, entity_iterator::{EntityIterator, EntityIteratorMut, PlayerIterator, PlayerIteratorMut}};

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

#[derive(Copy, Clone, PartialEq, Eq)]
pub struct EntityId {
    slab_index: usize,
    generation: usize
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub struct PlayerId {
    slab_index: usize,
    generation: usize
}

pub trait WorldExtension: Sized + 'static {
    type Universe: UniverseExtension;

    const CHUNKS_X: i32;
    const CHUNKS_Z: i32;
    const VIEW_DISTANCE: u8;
    const ENTITY_VIEW_DISTANCE: u8 = Self::VIEW_DISTANCE - 1;

    fn tick(world: &mut World<Self>);
}

pub struct World<W: WorldExtension> {
    universe: NonNull<Universe<W::Universe>>,

    players: Slab<Rc<UnsafeCell<dyn GenericPlayer>>>,
    entities: Slab<Rc<UnsafeCell<dyn GenericEntity>>>,
    pending_entities: Vec<Rc<UnsafeCell<dyn GenericEntity>>>,
    pub(crate) entities_by_network_id: HashMap<i32, Rc<UnsafeCell<dyn GenericEntity>>, BuildHasherDefault<FxHasher>>,
    next_generation: usize,

    chunks: Box<[Chunk]>,
    pub(crate) empty_chunk: Chunk,

    collision_aabb_buffer: RefCell<Vec<AABB>>,

    pub extension: W
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
            universe: universe.into(),

            players: Slab::new(),
            entities: Slab::new(),
            pending_entities: Vec::new(),
            entities_by_network_id: HashMap::default(),
            next_generation: 0,

            chunks: chunks.into_boxed_slice(),
            empty_chunk: Chunk::new_empty(chunk_list.size_y),

            collision_aabb_buffer: RefCell::new(Vec::new()),

            extension
        }
    }

    pub fn player<P: PlayerExtension + 'static>(&self, player_id: PlayerId) -> Option<&Player<P>> {
        unsafe { self.players.get(player_id.slab_index)?.get().as_mut() }.unwrap().downcast_ref::<Player<P>>()
            .filter(|player| player.self_id == Some(player_id))
    }

    pub fn player_mut<P: PlayerExtension + 'static>(&mut self, player_id: PlayerId) -> Option<&mut Player<P>> {
        unsafe { self.players.get_mut(player_id.slab_index)?.get().as_mut() }.unwrap().downcast_mut::<Player<P>>()
            .filter(|player| player.self_id == Some(player_id))
    }

    pub fn entity<E: EntityExtension + 'static>(&self, entity_id: EntityId) -> Option<&Entity<E>> {
        unsafe { self.entities.get(entity_id.slab_index)?.get().as_mut() }.unwrap().downcast_ref::<Entity<E>>()
            .filter(|entity| entity.self_id == Some(entity_id))
    }

    pub fn entity_mut<E: EntityExtension + 'static>(&mut self, entity_id: EntityId) -> Option<&mut Entity<E>> {
        unsafe { self.entities.get_mut(entity_id.slab_index)?.get().as_mut() }.unwrap().downcast_mut::<Entity<E>>()
            .filter(|entity| entity.self_id == Some(entity_id))
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

    pub fn get_block(&self, x: i32, y: i32, z: i32) -> Option<u16> {
        self.get_chunk(x >> 4, z >> 4)
            .and_then(|chunk| chunk.get_block(x, y, z))
    }

    pub fn spawn_new_entity<E: EntityExtension<World = W> + 'static>(&mut self, position: DVec3, extension: E) {
        let entity = Entity::new(self, position, extension);
        let entity = Rc::new(UnsafeCell::new(entity));
        self.pending_entities.push(entity);        
    }

    pub(crate) fn put_entity_into_chunk(&mut self, entity_id: EntityId, chunk_x: i32, chunk_z: i32) -> Option<ChunkEntityRef> {
        let entity = self.entities.get_mut(entity_id.slab_index).unwrap().clone();
        if let Some(chunk) = self.get_chunk_mut(chunk_x, chunk_z) {
            Some(chunk.insert_entity(entity))
        } else {
            None
        }
    }

    pub(crate) fn put_player_into_chunk(&mut self, player_id: PlayerId, chunk_x: i32, chunk_z: i32) -> Option<ChunkPlayerRef> {
        let player = self.players.get_mut(player_id.slab_index).unwrap().clone();
        if let Some(chunk) = self.get_chunk_mut(chunk_x, chunk_z) {
            Some(chunk.insert_player(player))
        } else {
            None
        }
    }

    pub fn spawn_new_player<P: PlayerExtension<World = W> + 'static>(&mut self, position: DVec3,
            player: ConfiguringPlayer<W::Universe>, extension: P) -> &mut Player<P> {
        let player = Player::new(self, position, player.connection.clone(), extension);
        let player_cell = Rc::new(UnsafeCell::new(player));

        let player_id = PlayerId {
            slab_index: self.players.insert(player_cell.clone()),
            generation: self.next_generation
        };
        self.next_generation += 1;

        let player_ref = unsafe { player_cell.get().as_mut() }.unwrap();

        player_ref.self_id = Some(player_id);
        player_ref.connection.as_ref().unwrap().borrow_mut().set_handler(player_cell.clone());

        let chunk_x = (position.x.floor() as i32) >> 4;
        let chunk_z = (position.z.floor() as i32) >> 4;
        if let Some(chunk) = self.get_chunk_mut(chunk_x, chunk_z) {
            let chunk_ref = chunk.insert_player(player_cell.clone());
            player_ref.chunk_ref = Some(chunk_ref);
        }

        // send join game
        let join_game = play::clientbound::JoinGame {
            entity_id: player_ref.entity_id,
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
            gamemode: 0,
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
                },
                TagRegistry {
                    tag_type: "minecraft:item",
                    values: vec![
                        Tag {
                            name: "minecraft:arrows",
                            entries: vec![Item::Arrow as u16]
                        },
                    ]
                },
                TagRegistry {
                    tag_type: "minecraft:block",
                    values: vec![
                        Tag {
                            name: "minecraft:climbable",
                            entries: vec![Block::CaveVines as u16, Block::CaveVinesPlant as u16, Block::Ladder as u16]
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

        player_ref.write_packet(&play::clientbound::SetChunkCacheCenter {
            chunk_x,
            chunk_z,
        }).unwrap();

        // write initial chunks
        let view_distance = W::VIEW_DISTANCE as i32;
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
        let view_distance = W::ENTITY_VIEW_DISTANCE as i32;
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
        player_ref
    }

    pub fn players<P: PlayerExtension>(&self) -> PlayerIterator<'_, P> {
        let empty = std::any::TypeId::of::<P::World>() != std::any::TypeId::of::<W>();
        PlayerIterator::new(self.players.iter(), empty)
    }

    pub fn players_mut<P: PlayerExtension>(&mut self) -> PlayerIteratorMut<'_, P> {
        let empty = std::any::TypeId::of::<P::World>() != std::any::TypeId::of::<W>();
        PlayerIteratorMut::new(self.players.iter_mut(), empty)
    }

    pub fn entities<E: EntityExtension>(&self) -> EntityIterator<'_, E> {
        let empty = std::any::TypeId::of::<E::World>() != std::any::TypeId::of::<W>();
        EntityIterator::new(self.entities.iter(), empty)
    }

    pub fn entities_mut<E: EntityExtension>(&mut self) -> EntityIteratorMut<'_, E> {
        let empty = std::any::TypeId::of::<E::World>() != std::any::TypeId::of::<W>();
        EntityIteratorMut::new(self.entities.iter_mut(), empty)
    }

    pub fn spawn_particle(&mut self, x: f64, y: f64, z: f64, particle: Particle) {
        let chunk_x = (x.floor() as i32) >> 4;
        let chunk_z = (z.floor() as i32) >> 4;

        if let Some(chunk) = self.get_chunk_mut(chunk_x, chunk_z) {
            chunk.chunk_viewable.write_packet(&LevelParticles {
                particle_id: particle.get_id(),
                long_distance: true,
                x,
                y,
                z,
                offset_x: 0.0,
                offset_y: 0.0,
                offset_z: 0.0,
                max_speed: 0.0,
                particle_count: 0,
                extra_data: Cow::Borrowed(&[])
            }).unwrap();
        }
    }

    pub fn spawn_debug_particle(&mut self, x: f64, y: f64, z: f64) {
        self.spawn_particle(x, y, z, Particle::Composter)
    }

    pub fn move_bounding_box_with_collision(&self, mut aabb: AABB, mut delta: DVec3) -> (DVec3, bool, bool, bool) {
        const EPSILON: f64 = 1E-7;

        let Some(mut normalized) = delta.try_normalize() else {
            return (DVec3::ZERO, false, false, false);
        };

        let expanded = aabb.expand(delta);
        let broad_phase_min = (expanded.min() - EPSILON).floor().as_ivec3() - 1;
        let broad_phase_max = (expanded.max() + EPSILON).floor().as_ivec3() + 1;

        let mut travelled = DVec3::ZERO;

        let mut buffer = self.collision_aabb_buffer.borrow_mut();

        buffer.clear();

        for x in broad_phase_min.x..broad_phase_max.x+1 {
            for y in broad_phase_min.y..broad_phase_max.y+1 {
                for z in broad_phase_min.z..broad_phase_max.z+1 {
                    if let Some(chunk) = self.get_chunk(x >> 4, z >> 4) {
                        if let Some(block) = chunk.get_block(x, y, z) {
                            if block == 0 {
                                continue;
                            }

                            let attr: &BlockAttributes = block.try_into().unwrap();
                            if attr.solid {
                                let aabb = AABB::new(DVec3::new(x as f64 + EPSILON, y as f64 + EPSILON, z as f64 + EPSILON),
                                    DVec3::new(x as f64 + 1.0 - EPSILON, y as f64 + 1.0 - EPSILON, z as f64 + 1.0 - EPSILON)).unwrap();
                                    buffer.push(aabb);
                            }
                        }
                    }
                }
            }
        }

        if buffer.is_empty() {
            return (delta, false, false, false);
        }

        let mut collided_x = false;
        let mut collided_y = false;
        let mut collided_z = false;

        loop {
            let mut t = delta.length();

            let mut min_hit_sides = 0;
            let mut hit = 0;

            for block_aabb in buffer.iter() {                   
                let minkowski_difference = aabb.minkowski_difference(*block_aabb);
        
                if let Some((new_t, new_hit)) = ray_box(-normalized, minkowski_difference) {
                    if new_t < t {
                        t = new_t;
                        hit = new_hit;
                        min_hit_sides = new_hit.count_ones();
                    } else if new_t == t {
                        let new_min_hit_sides = new_hit.count_ones();
                        if new_min_hit_sides < min_hit_sides {
                            min_hit_sides = new_min_hit_sides;
                            hit = new_hit;
                        } else if new_min_hit_sides == min_hit_sides {
                            hit |= new_hit;
                        }
                    }
                }
            }

            if hit == 0 {
                buffer.clear();
                return (travelled + delta, collided_x, collided_y, collided_z);
            } else {
                // todo: better solution to prevent precision issues than multiplying by 0.999
                let to_collision = normalized * t;

                travelled += to_collision;
                delta -= to_collision;

                if min_hit_sides == 0 { // This shouldn't be possible
                    buffer.clear();
                    return (travelled, collided_x, collided_y, collided_z);
                } else if min_hit_sides == 1 {
                    // Hit the side(s) of blocks
                    if (hit & (1 << 0)) != 0 { // X
                        if normalized.x > 0.0 {
                            travelled.x -= EPSILON * 2.0;
                        } else {
                            travelled.x += EPSILON * 2.0;
                        }
                        delta[0] = 0.0;
                        collided_x = true;
                    }
                    if (hit & (1 << 1)) != 0 { // Y
                        if normalized.y > 0.0 {
                            travelled.y -= EPSILON * 2.0;
                        } else {
                            travelled.y += EPSILON * 2.0;
                        }
                        delta[1] = 0.0;
                        collided_y = true;
                    }
                    if (hit & (1 << 2)) != 0 { // Z
                        if normalized.z > 0.0 {
                            travelled.z -= EPSILON * 2.0;
                        } else {
                            travelled.z += EPSILON * 2.0;
                        }
                        delta[2] = 0.0;
                        collided_z = true;
                    }
                } else {
                    // Hit the corner of a block, choose which axis to negate in order XZY
                    if (hit & (1 << 0)) != 0 { // X
                        if normalized.x > 0.0 {
                            travelled.x -= EPSILON * 2.0;
                        } else {
                            travelled.x += EPSILON * 2.0;
                        }
                        delta[0] = 0.0;
                        collided_x = true;
                    } else if (hit & (1 << 2)) != 0 { // Z
                        if normalized.z > 0.0 {
                            travelled.z -= EPSILON * 2.0;
                        } else {
                            travelled.z += EPSILON * 2.0;
                        }
                        delta[2] = 0.0;
                        collided_y = true;
                    } else { // Y
                        if normalized.y > 0.0 {
                            travelled.y -= EPSILON * 2.0;
                        } else {
                            travelled.y += EPSILON * 2.0;
                        }
                        delta[1] = 0.0;
                        collided_z = true;
                    }
                }

                aabb = AABB::new(
                    aabb.min() + to_collision,
                    aabb.max() + to_collision
                ).unwrap();

                if let Some(new_normalized) = delta.try_normalize() {
                    normalized = new_normalized;
                } else {
                    buffer.clear();
                    return (travelled, collided_x, collided_y, collided_z);
                }
            }

        }
    }

    fn tick(&mut self) {
        // Remove all players that have disconnected
        self.players.retain(|_, player| unsafe { player.get().as_mut() }.unwrap().is_valid());

        // Add any pending entities
        for new_entity in self.pending_entities.drain(..) {
            let index = self.entities.insert(new_entity.clone());
            let entity_mut = unsafe { new_entity.get().as_mut() }.unwrap();
            let entity_id = EntityId {
                slab_index: index,
                generation: self.next_generation
            };
            self.next_generation += 1;

            entity_mut.add_to_world(entity_id, new_entity);
        }

        W::tick(self);

        // Tick players
        for (_, player) in &mut self.players {
            unsafe { player.get().as_mut() }.unwrap().tick();
        }

        // Tick or remove entities
        self.entities.retain(|idx, entity| {
            let entity_ref = unsafe { entity.get().as_mut() }.unwrap();
            if let Some(id) = entity_ref.get_self_id() {
                if id.slab_index == idx {
                    entity_ref.tick();
                    return true;
                }
            }

            if Rc::strong_count(entity) > 1 {
                panic!("Possible memory leak: entity had rc count > 1 when being removed");
            }

            false
        });

        // Add any pending entities
        for new_entity in self.pending_entities.drain(..) {
            let index = self.entities.insert(new_entity.clone());
            let entity_mut = unsafe { new_entity.get().as_mut() }.unwrap();
            let entity_id = EntityId {
                slab_index: index,
                generation: self.next_generation
            };
            self.next_generation += 1;

            entity_mut.add_to_world(entity_id, new_entity);
        }

        // View tick player
        for (_, player) in &mut self.players {
            unsafe { player.get().as_mut() }.unwrap().view_tick();
        }

        // Clear viewable buffer on chunk
        for chunk in self.chunks.iter_mut() {
            chunk.clear_viewable_packets();
        }
    }
}

fn ray_box(ray: DVec3, aabb: AABB) -> Option<(f64, u8)> {
    let mut t_near = f64::MIN;
    let mut t_far = f64::MAX;
    let mut hit = 0;

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
                hit = 1 << i;
            } else if near == t_near {
                hit |= 1 << i;
            }

            if t_near > t_far {
                return None;
            }
        }
    }

    if t_near >= 0.0 {
        Some((t_near, hit))
    } else {
        None
    }
}