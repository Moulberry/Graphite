use std::{borrow::Cow, cell::{RefCell, UnsafeCell}, collections::{hash_map::Entry, HashMap}, hash::BuildHasherDefault, net::SocketAddr, ops::ControlFlow, rc::Rc, sync::{atomic::{AtomicBool, Ordering}, Arc}, time::Duration};

use downcast_rs::Downcast;
use enumset::EnumSet;
use glam::{DVec2, DVec3, Vec3Swizzles};
use graphite_binary::nbt::EncodedNBT;
use graphite_mc_constants::{block::{Block, BlockAttributes, BlockFlag}, entity::EntityAndMetadata, item::Item, particle::Particle, types::SoundSource};
use graphite_mc_protocol::{play::{self, clientbound::{GameEventType, LevelParticles, PlayerInfoAction, PlayerInfoEntry, Sound, Tag, TagRegistry}}, types::{text::TextComponent, GameProfile, SoundType}, IdentifiedPacket};
use graphite_network::{Connection, NetworkHandlerService, ServiceTickAction, ConnectionSender};
use hecs::{EntityBuilder, QueryBorrow, QueryOne};

use rand::RngCore;
use rustc_hash::FxHasher;
use slab::Slab;

use crate::{entity::{entity_view::EntityView, remote_entity::RemoteEntity, Collision, EntityBase}, player::{GenericPlayer, Player, PlayerExtension}, registry::Registries, types::AABB, world::pathfinding_cache::PathfindingCacheHolder};

use super::{chunk::{Chunk, ChunkPlayerRef, ChunkProvider}, chunk_iterator::{NearbyChunkIter, NearbyChunkIterMut}, chunk_list::ChunkList, chunk_section::ChunkSection, inbound_player::InboundPlayer, outbound_player::OutboundPlayer, player_iterator::{NearbyPlayerIterMut, NearbyPlayerIterator, PlayerIterator, PlayerIteratorMut}};

thread_local! {
    static COLLISION_AABB_BUFFER: RefCell<Vec<AABB>> = RefCell::new(Vec::new());
}

pub trait BlockGetter {
    fn get_block(&self, x: i32, y: i32, z: i32) -> Option<u16>;
    fn get_chunk_section(&self, chunk_x: i32, chunk_y: i32, chunk_z: i32) -> Option<&ChunkSection>;
    fn chunks_x(&self) -> usize;
    fn chunks_y(&self) -> usize;
    fn chunks_z(&self) -> usize;

    fn shape_cast(&self, aabb: AABB, delta: DVec3) -> (DVec3, bool, bool, bool) {
        self.shape_cast_with_water_solid(aabb, delta, false)
    }

    #[allow(unused)]
    fn add_additional_collisions_for_chunk(&self, chunk_x: i32, chunk_z: i32, buffer: &mut Vec<AABB>) {
    }

    fn shape_cast_with_water_solid(&self, mut aabb: AABB, mut delta: DVec3, is_water_solid: bool) -> (DVec3, bool, bool, bool) {
        const EPSILON: f64 = 1E-7;

        let Some(mut normalized) = delta.try_normalize() else {
            return (DVec3::ZERO, false, false, false);
        };

        let expanded = aabb.expand(delta);
        let broad_phase_min = (expanded.min() - EPSILON).floor().as_ivec3() - 1;
        let broad_phase_max = (expanded.max() + EPSILON).floor().as_ivec3() + 1;

        let mut travelled = DVec3::ZERO;

        COLLISION_AABB_BUFFER.with(|buffer| {
            let mut buffer = buffer.borrow_mut();
            buffer.clear();

            let min_chunk_x = ((broad_phase_min.x >> 4) - 1).max(0);
            let min_chunk_z = ((broad_phase_min.z >> 4) - 1).max(0);
            let max_chunk_x = ((broad_phase_max.x >> 4) + 1).min(self.chunks_x() as i32 - 1);
            let max_chunk_z = ((broad_phase_max.z >> 4) + 1).min(self.chunks_z() as i32 - 1);

            for chunk_x in min_chunk_x..max_chunk_x+1 {
                for chunk_z in min_chunk_z..max_chunk_z+1 {
                    self.add_additional_collisions_for_chunk(chunk_x, chunk_z, &mut *buffer);
                }
            }

            for x in broad_phase_min.x..broad_phase_max.x+1 {
                for y in broad_phase_min.y..broad_phase_max.y+1 {
                    for z in broad_phase_min.z..broad_phase_max.z+1 {
                        if let Some(block) = self.get_block(x, y, z) {
                            if block == 0 {
                                continue;
                            }

                            let attr = BlockAttributes::from_block_state(block);
                            if is_water_solid && attr.has_flag(BlockFlag::Waterlogged) {
                                buffer.push(AABB::new(
                                    DVec3::new(
                                        x as f64 + EPSILON, 
                                        y as f64 + EPSILON, 
                                        z as f64 + EPSILON
                                    ),
                                    DVec3::new(
                                        x as f64 + 1.0 - EPSILON, 
                                        y as f64 + 1.0 - EPSILON, 
                                        z as f64 + 1.0 - EPSILON
                                    )
                                ));
                            } else {
                                for aabb in attr.collision_shape {
                                    buffer.push(AABB::new(
                                        DVec3::new(
                                            x as f64 + aabb[0] + EPSILON, 
                                            y as f64 + aabb[1] + EPSILON, 
                                            z as f64 + aabb[2] + EPSILON
                                        ),
                                        DVec3::new(
                                            x as f64 + aabb[3] - EPSILON, 
                                            y as f64 + aabb[4] - EPSILON, 
                                            z as f64 + aabb[5] - EPSILON
                                        )
                                    ));
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

            let original_target = delta;

            loop {
                let mut t = delta.length();

                let mut min_hit_sides = 0;
                let mut hit = 0;

                for block_aabb in buffer.iter() {                   
                    let minkowski_difference = aabb.minkowski_difference(block_aabb);
            
                    if let Some((new_t, new_hit)) = minkowski_difference.ray_box(-normalized) {
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
                    if travelled == DVec3::ZERO {
                        buffer.clear();
                        return (delta, collided_x, collided_y, collided_z);
                    } else {
                        travelled += delta;
                        aabb = AABB::new(
                            aabb.min() + delta,
                            aabb.max() + delta
                        );

                        do_simple_shape_cast(&mut buffer, aabb, original_target,
                            &mut collided_x, &mut collided_y, &mut collided_z, &mut travelled);
                        buffer.clear();
                        return (travelled, collided_x, collided_y, collided_z);
                    }
                } else {
                    let to_collision = normalized * t;

                    travelled += to_collision;
                    delta -= to_collision;

                    if min_hit_sides == 0 { // This shouldn't be possible
                        buffer.clear();
                        return (travelled, collided_x, collided_y, collided_z);
                    } else if min_hit_sides == 1 {
                        // Hit the side(s) of blocks
                        if (hit & (1 << 0)) != 0 { // X
                            travelled.x -= normalized.x.signum() * EPSILON * 2.0;
                            delta.x = 0.0;
                            collided_x = true;
                        }
                        if (hit & (1 << 1)) != 0 { // Y
                            travelled.y -= normalized.y.signum() * EPSILON * 2.0;
                            delta.y = 0.0;
                            collided_y = true;
                        }
                        if (hit & (1 << 2)) != 0 { // Z
                            travelled.z -= normalized.z.signum() * EPSILON * 2.0;
                            delta.z = 0.0;
                            collided_z = true;
                        }
                    } else {
                        // Back off from wall
                        if (hit & (1 << 0)) != 0 { // X
                            travelled.x -= normalized.x.signum() * EPSILON * 2.0;
                        }
                        if (hit & (1 << 1)) != 0 { // Y
                            travelled.y -= normalized.y.signum() * EPSILON * 2.0;
                        }
                        if (hit & (1 << 2)) != 0 { // Z
                            travelled.z -= normalized.z.signum() * EPSILON * 2.0;
                        }

                        // Hit the edge or corner of a block, only negate one axis in order XZY
                        if (hit & (1 << 0)) != 0 { // X
                            delta.x = 0.0;
                            collided_x = true;
                        } else if (hit & (1 << 2)) != 0 { // Z
                            delta.z = 0.0;
                            collided_z = true;
                        } else { // Y
                            delta.y = 0.0;
                            collided_y = true;
                        }
                    }

                    aabb = AABB::new(
                        aabb.min() + to_collision,
                        aabb.max() + to_collision
                    );

                    if let Some(new_normalized) = delta.try_normalize() {
                        normalized = new_normalized;
                    } else {
                        do_simple_shape_cast(&mut buffer, aabb, original_target,
                            &mut collided_x, &mut collided_y, &mut collided_z, &mut travelled);

                        buffer.clear();
                        return (travelled, collided_x, collided_y, collided_z);
                    }
                }
            }
        })
    }
}

fn do_simple_shape_cast(buffer: &mut Vec<AABB>, aabb: AABB, original_target: DVec3,
        collided_x: &mut bool, collided_y: &mut bool, collided_z: &mut bool, travelled: &mut DVec3) {
    let remaining_delta = original_target - *travelled;
    if remaining_delta.length_squared() <= 0.01*0.01 {
        return;
    }

    let mut t = remaining_delta.length();
    let mut hit = 0;

    let normalized = remaining_delta.normalize();
    for block_aabb in buffer.iter() {                   
        let minkowski_difference = aabb.minkowski_difference(block_aabb);
                        
        if let Some((new_t, new_hit)) = minkowski_difference.ray_box(-normalized) {
            if new_t < t {
                t = new_t;
                hit = new_hit;
            } else if new_t == t {
                hit |= new_hit;
            }
        }
    }
    if t > 0.01 {
        *travelled += normalized * t;

        // Back off from wall
        const EPSILON: f64 = 1E-7;
        if (hit & (1 << 0)) != 0 { // X
            travelled.x -= normalized.x.signum() * EPSILON * 2.0;
            *collided_x = true;
        }
        if (hit & (1 << 1)) != 0 { // Y
            travelled.y -= normalized.y.signum() * EPSILON * 2.0;
            *collided_y = true;
        }
        if (hit & (1 << 2)) != 0 { // Z
            travelled.z -= normalized.z.signum() * EPSILON * 2.0;
            *collided_z = true;
        }
    }
}

pub trait LightSetter {
    fn set_block_light_array(&mut self, section_x: usize, section_y: usize, section_z: usize, light: Box<[u8]>);
    fn set_sky_light_array(&mut self, section_x: usize, section_y: usize, section_z: usize, light: Box<[u8]>);
}

pub struct SingleQuery<'a, Q: hecs::Query> {
    query: Option<QueryOne<'a, Q>>
}

impl <'a, Q: hecs::Query> SingleQuery<'a, Q> {
    pub fn get(&mut self) -> Option<Q::Item<'_>> {
        if let Some(query) = &mut self.query {
            query.get()
        } else {
            None
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct PlayerId {
    slab_index: usize,
    generation: usize
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct RemoteEntityId {
    pub(crate) slab_index: usize,
    generation: usize
}

pub trait WorldExtension: Sized + 'static {
    type JoinData: TryFrom<SocketAddr> + Send;

    const VIEW_DISTANCE: u8;
    const ENTITY_VIEW_DISTANCE: u8 = Self::VIEW_DISTANCE - 1;

    fn tick(world: &mut World<Self>);
    fn on_player_join(world: &mut World<Self>, data: Self::JoinData, connection: Rc<RefCell<Connection>>);
}

pub struct WorldController<W: WorldExtension> {
    pub terminate: Arc<AtomicBool>,
    pub terminated: Arc<AtomicBool>,
    pub join: ConnectionSender<W::JoinData>
}

pub struct World<W: WorldExtension> {
    inbound_players: Vec<Rc<UnsafeCell<InboundPlayer<W>>>>,
    outbound_players: Vec<Rc<UnsafeCell<OutboundPlayer>>>,
    players: Slab<Rc<UnsafeCell<dyn GenericPlayer>>>,
    player_generation: usize,

    registries: Registries,

    entities_for_removal: Vec<hecs::Entity>,

    terminate: Option<Arc<AtomicBool>>,
    terminated: Option<Arc<AtomicBool>>,

    entities: hecs::World,
    pending_entities: Vec<(hecs::Entity, EntityBuilder)>,

    pub(crate) remote_entities: Slab<RemoteEntity>,
    remote_entity_generation: usize,
    pub(crate) remote_entity_by_network_id: HashMap<i32, usize, BuildHasherDefault<FxHasher>>,

    pub chunks: Box<[Chunk]>,
    pub chunks_x: i32,
    pub chunks_z: i32,
    pub(crate) empty_chunk: Chunk,

    pub pathfinding_cache: PathfindingCacheHolder,

    pub extension: W
}

pub trait GenericWorld: Downcast + ChunkProvider + BlockGetter {
    fn entity_world(&self) -> &hecs::World;
    fn spawn_entity(&mut self, position: DVec3, builder: EntityBuilder) -> hecs::Entity {
        self.spawn_entity_with_velocity(position, DVec3::ZERO, builder)
    }
    fn spawn_entity_with_rotation(&mut self, position: DVec3, yaw: f64, pitch: f64, builder: EntityBuilder) -> hecs::Entity;
    fn spawn_entity_with_velocity(&mut self, position: DVec3, velocity: DVec3, builder: EntityBuilder) -> hecs::Entity;

    fn register_remote_entity(&mut self, remote_entity: RemoteEntity) -> RemoteEntityId;
    fn deregister_remote_entity(&mut self, remote_entity_id: RemoteEntityId) -> Option<RemoteEntity>;
    fn get_remote_entity(&self, id: RemoteEntityId) -> Option<&RemoteEntity>;
    fn get_remote_entity_mut(&mut self, id: RemoteEntityId) -> Option<&mut RemoteEntity>;
    fn get_remote_entities(&self) -> &Slab<RemoteEntity>;

    fn spawn_particle(&mut self, x: f64, y: f64, z: f64, particle: Particle);
    fn spawn_particles(&mut self, x: f64, y: f64, z: f64, offset_x: f32, offset_y: f32,
        offset_z: f32, max_speed: f32, particle_count: i32, particle: Particle);

    fn play_sound<'a>(&mut self, position: DVec3, sound: SoundType<'a>, source: SoundSource) {
        self.play_sound_with_levels(position, sound, source, 1.0, 1.0)
    }
    fn play_sound_with_levels<'a>(&mut self, position: DVec3, sound: SoundType<'a>, source: SoundSource, volume: f32, pitch: f32);

    fn nearby_chunks(&self, position: DVec2, distance: f64) -> NearbyChunkIter;
    fn nearby_chunks_mut(&mut self, position: DVec2, distance: f64) -> NearbyChunkIterMut;

    fn disconnect_all(&mut self, message: Option<EncodedNBT>);

    fn terminate(&mut self);

    fn has_players(&self) -> bool;

    fn raycast_generic_bool(&self, from: DVec3, to: DVec3, mut func: Box<dyn FnMut(i32, i32, i32, u16) -> ControlFlow<bool>>) -> Option<bool> {
        let delta = to - from;
        let direction = delta.try_normalize()?;

        let mut map_x = from.x.floor() as i32;
        let mut map_y = from.y.floor() as i32;
        let mut map_z = from.z.floor() as i32;

        let to_map_x = to.x.floor() as i32;
        let to_map_y = to.y.floor() as i32;
        let to_map_z = to.z.floor() as i32;

        let block = self.get_block(map_x, map_y, map_z)?;
        if let ControlFlow::Break(v) = (func)(map_x, map_y, map_z, block) {
            return Some(v);
        }

        if map_x == to_map_x && map_y == to_map_y && map_z == to_map_z {
            return None;
        }

        let delta_dist_x = (1.0 / direction.x).abs();
        let delta_dist_y = (1.0 / direction.y).abs();
        let delta_dist_z = (1.0 / direction.z).abs();

        let (step_x, mut side_dist_x) = if direction.x > 0.0 {
            (1, 1.0 - from.x + map_x as f64)
        } else {
            (-1, from.x - map_x as f64)
        };
        let (step_y, mut side_dist_y) = if direction.y > 0.0 {
            (1, 1.0 - from.y + map_y as f64)
        } else {
            (-1, from.y - map_y as f64)
        };
        let (step_z, mut side_dist_z) = if direction.z > 0.0 {
            (1, 1.0 - from.z + map_z as f64)
        } else {
            (-1, from.z - map_z as f64)
        };

        side_dist_x *= delta_dist_x;
        side_dist_y *= delta_dist_y;
        side_dist_z *= delta_dist_z;

        if side_dist_x.is_nan() {
            side_dist_x = f64::INFINITY;
        }
        if side_dist_y.is_nan() {
            side_dist_y = f64::INFINITY;
        }
        if side_dist_z.is_nan() {
            side_dist_z = f64::INFINITY;
        }

        loop {
            if side_dist_z < side_dist_x && side_dist_z < side_dist_y {
                side_dist_z += delta_dist_z;
                map_z += step_z;
                if map_z*step_z > to_map_z*step_z {
                    return None;
                }
            } else if side_dist_x < side_dist_y {
                side_dist_x += delta_dist_x;
                map_x += step_x;
                if map_x*step_x > to_map_x*step_x {
                    return None;
                }
            } else {
                side_dist_y += delta_dist_y;
                map_y += step_y;
                if map_y*step_y > to_map_y*step_y {
                    return None;
                }
            }

            let block = self.get_block(map_x, map_y, map_z)?;
            if let ControlFlow::Break(v) = (func)(map_x, map_y, map_z, block) {
                return Some(v);
            }
        }
    }
        
    // Use EntityBase::remove instead of this
    #[doc(hidden)]
    fn mark_for_removal(&mut self, entity_id: hecs::Entity);

    #[doc(hidden)]
    unsafe fn entity_world_mut(&mut self) -> &mut hecs::World;
}
downcast_rs::impl_downcast!(GenericWorld);

impl <W: WorldExtension + 'static> ChunkProvider for World<W> {
    fn get_chunk(&self, x: i32, z: i32) -> Option<&Chunk> {
        if x < 0 || z < 0 || x >= self.chunks_x || z >= self.chunks_z {
            None
        } else {
            Some(&self.chunks[(x + z * self.chunks_x) as usize])
        }
    }

    fn get_chunk_mut(&mut self, x: i32, z: i32) -> Option<&mut Chunk> {
        if x < 0 || z < 0 || x >= self.chunks_x || z >= self.chunks_z {
            None
        } else {
            Some(&mut self.chunks[(x + z * self.chunks_x) as usize])
        }
    }
}

impl <W: WorldExtension + 'static> BlockGetter for World<W> {
    fn get_block(&self, x: i32, y: i32, z: i32) -> Option<u16> {
        self.get_chunk(x >> 4, z >> 4)
            .and_then(|chunk| chunk.get_block(x, y, z))
    }

    fn get_chunk_section(&self, chunk_x: i32, chunk_y: i32, chunk_z: i32) -> Option<&ChunkSection> {
        self.get_chunk(chunk_x, chunk_z)
            .and_then(|chunk| chunk.get_section(chunk_y))
    }
    
    fn chunks_x(&self) -> usize {
        self.chunks_x as usize
    }

    fn chunks_y(&self) -> usize {
        self.empty_chunk.section_count()
    }
    
    fn chunks_z(&self) -> usize {
        self.chunks_z as usize
    }

    fn add_additional_collisions_for_chunk(&self, chunk_x: i32, chunk_z: i32, buffer: &mut Vec<AABB>) {
        buffer.extend(self.chunks[(chunk_x + chunk_z * self.chunks_x) as usize].solid_entity_aabbs.iter());
    }
}

impl <W: WorldExtension + 'static> LightSetter for World<W> {
    fn set_block_light_array(&mut self, section_x: usize, section_y: usize, section_z: usize, light: Box<[u8]>) {
        if let Some(chunk) = self.get_chunk_mut(section_x as i32, section_z as i32) {
            chunk.set_block_light_array(section_y, light);
        }
    }

    fn set_sky_light_array(&mut self, section_x: usize, section_y: usize, section_z: usize, light: Box<[u8]>) {
        if let Some(chunk) = self.get_chunk_mut(section_x as i32, section_z as i32) {
            chunk.set_sky_light_array(section_y, light);
        }
    }
}

impl <W: WorldExtension + 'static> GenericWorld for World<W> {
    fn entity_world(&self) -> &hecs::World {
        &self.entities
    }

    fn spawn_entity_with_rotation(&mut self, position: DVec3, yaw: f64, pitch: f64, mut builder: EntityBuilder) -> hecs::Entity {
        if let Some(base) = builder.get_mut::<&mut EntityBase>() {
            base.position = position;
            base.rotation = DVec2::new(pitch, yaw);
        } else {
            builder.add(EntityBase::new(position, DVec3::ZERO, DVec2::new(pitch, yaw)));
        }
        let id = self.entities.reserve_entity();
        self.pending_entities.push((id, builder));
        id
    }

    fn spawn_entity_with_velocity(&mut self, position: DVec3, velocity: DVec3, mut builder: EntityBuilder) -> hecs::Entity {
        if let Some(base) = builder.get_mut::<&mut EntityBase>() {
            base.position = position;
            base.velocity = velocity;
        } else {
            builder.add(EntityBase::new(position, velocity, DVec2::ZERO));
        }
        let id = self.entities.reserve_entity();
        self.pending_entities.push((id, builder));
        id
    }

    fn register_remote_entity(&mut self, mut remote_entity: RemoteEntity) -> RemoteEntityId {
        let vacant = self.remote_entities.vacant_entry();
        let index = vacant.key();

        // Create and set RemoteEntityId
        self.remote_entity_generation += 1;
        let id = RemoteEntityId {
            slab_index: index,
            generation: self.remote_entity_generation
        };
        remote_entity.id = Some(id);

        // Put entity
        let network_id = remote_entity.network_id();
        vacant.insert(remote_entity);
        self.remote_entity_by_network_id.insert(network_id, index);

        id   
    }

    fn deregister_remote_entity(&mut self, remote_entity_id: RemoteEntityId) -> Option<RemoteEntity> {
        let remote_entity = &self.remote_entities[remote_entity_id.slab_index];
        if remote_entity.id != Some(remote_entity_id) {
            return None;
        }
        self.remote_entity_by_network_id.remove(&remote_entity.network_id());
        Some(self.remote_entities.remove(remote_entity_id.slab_index))
    }

    fn get_remote_entity(&self, id: RemoteEntityId) -> Option<&RemoteEntity> {
        self.remote_entities.get(id.slab_index).filter(|entity| entity.id.unwrap() == id)
    }

    fn get_remote_entity_mut(&mut self, id: RemoteEntityId) -> Option<&mut RemoteEntity> {
        self.remote_entities.get_mut(id.slab_index).filter(|entity| entity.id.unwrap() == id)
    }

    fn get_remote_entities(&self) -> &Slab<RemoteEntity> {
        &self.remote_entities
    }

    fn spawn_particle(&mut self, x: f64, y: f64, z: f64, particle: Particle) {
        self.spawn_particle(x, y, z, particle)
    }

    fn spawn_particles(&mut self, x: f64, y: f64, z: f64, offset_x: f32, offset_y: f32,
            offset_z: f32, max_speed: f32, particle_count: i32, particle: Particle) {
        self.spawn_particles(x, y, z, offset_x, offset_y, offset_z, max_speed, particle_count, particle)
    }

    fn play_sound_with_levels<'a>(&mut self, position: DVec3, sound: SoundType<'a>, source: SoundSource, volume: f32, pitch: f32) {
        let chunk_x = (position.x.floor() as i32) >> 4;
        let chunk_z = (position.z.floor() as i32) >> 4;

        if let Some(chunk) = self.get_chunk_mut(chunk_x, chunk_z) {
            Sound {
                sound,
                source,
                x: position.x as f32,
                y: position.y as f32,
                z: position.z as f32,
                volume,
                pitch,
                seed: rand::thread_rng().next_u64(),
            }.write_packet(&mut chunk.entity_viewable);
        }        
    }

    fn nearby_chunks(&self, position: DVec2, distance: f64) -> NearbyChunkIter {
        NearbyChunkIter::new(&self.chunks, self.chunks_x, self.chunks_z, position, distance)
    }

    fn nearby_chunks_mut(&mut self, position: DVec2, distance: f64) -> NearbyChunkIterMut {
        NearbyChunkIterMut::new(&mut self.chunks, self.chunks_x, self.chunks_z, position, distance)
    }

    fn disconnect_all(&mut self, message: Option<EncodedNBT>) {
        for (_, player) in &self.players {
            let player = unsafe { player.get().as_mut() }.unwrap();
            player.disconnect(message.clone());
        }
        for player in &self.inbound_players {
            let player = unsafe { player.get().as_mut() }.unwrap();
            player.disconnect(message.clone());
        }
        for player in &self.outbound_players {
            let player = unsafe { player.get().as_mut() }.unwrap();
            player.disconnect(message.clone());
        }
    }

    fn terminate(&mut self) {
        self.terminate.as_mut().expect("terminate is set").store(true, Ordering::Relaxed);
    }

    fn has_players(&self) -> bool {
        !self.players.is_empty() || !self.inbound_players.is_empty() || !self.outbound_players.is_empty()
    }

    fn mark_for_removal(&mut self, entity_id: hecs::Entity) {
        self.entities_for_removal.push(entity_id);
    }

    unsafe fn entity_world_mut(&mut self) -> &mut hecs::World {
        &mut self.entities
    }
}

impl <W: WorldExtension + 'static> NetworkHandlerService for Box<World<W>> {
    const MAXIMUM_PACKET_SIZE: usize = 2097151;
    const TICK_RATE: Option<std::time::Duration> = Some(Duration::from_millis(50));
    type ExtraData = W::JoinData;

    fn accept_new_connection(&mut self, extra_data: Self::ExtraData, connection: Rc<RefCell<Connection>>) {
        let configuring_player = InboundPlayer::new(extra_data, connection);

        let player_ref = unsafe { configuring_player.get().as_mut() }.unwrap();
        player_ref.send_registries(&self.registries);
        player_ref.send_finish();

        self.inbound_players.push(configuring_player);
    }

    fn tick(&mut self) -> ServiceTickAction {
        if let Some(terminate) = &self.terminate {
            if terminate.load(Ordering::Relaxed) {
                self.disconnect_all(Some(TextComponent::literal("Server closed").to_encoded_nbt()));
                if let Some(terminated) = &self.terminated {
                    terminated.store(true, Ordering::Relaxed);
                }
                return ServiceTickAction::Shutdown;
            }
        }

        self.handle_inbound_players();
        self.handle_outbound_players();
        
        let mut removed_player_uuids = Vec::new();
        let mut removed_player_entity_ids = Vec::new();

        // Remove all players that have disconnected
        self.handle_player_updates(&mut removed_player_uuids, &mut removed_player_entity_ids, false);

        // Tick the world
        self.process_entity_pending();
        W::tick(self);
        self.process_entity_pending();

        // Tick the player
        self.handle_player_updates(&mut removed_player_uuids, &mut removed_player_entity_ids, true);

        // Invalidate pathfinding cache
        self.pathfinding_cache.invalidate();

        // Update entities
        self.process_entity_pending();
        for entity in self.entities.iter() {
            let mut base = entity.get::<&mut EntityBase>().unwrap();
            if let Some(mut collision) = entity.get::<&mut Collision>() {
                base.update_with_collision(&mut *collision);
            } else {
                base.update_without_collision();
            }
            if let Some(mut view) = entity.get::<&mut EntityView>() {
                base.update_position_with_view(&mut *view);
                (view.update)(entity, &mut *base, &mut *view);
            } else {
                base.update_position_without_view();
            }

            let (chunk_x, chunk_z) = base.last_chunk_coords();
            if chunk_x >= 0 && chunk_z >= 0 && chunk_x < self.chunks_x && chunk_z < self.chunks_z {
                let chunk = &mut self.chunks[(chunk_x + chunk_z * self.chunks_x) as usize];
                for &remote_entity in &base.remote_entities {
                    let remote_entity = &mut self.remote_entities[remote_entity.slab_index];
                    remote_entity.tick(chunk, base.position, base.rotation.y, base.rotation.x);
                }
            }
        }
        self.process_entity_pending();

        // Create player removal packets, if any
        let removed_player_packets = if removed_player_uuids.is_empty() {
            None
        } else {
            Some((
                play::clientbound::PlayerInfoRemove {
                    players: removed_player_uuids
                },
                play::clientbound::RemoveEntities {
                    entities: Cow::Owned(removed_player_entity_ids),
                }
            ))
        };

        // View tick player
        for (_, player) in &mut self.players {
            let player = unsafe { player.get().as_mut() }.unwrap();

            player.view_tick();

            // Write remove players
            if let Some((info_remove, remove_entities)) = &removed_player_packets {
                remove_entities.write_packet(player.get_packet_buffer());
                info_remove.write_packet(player.get_packet_buffer());
            }
        }

        // Clear viewable buffer on chunk
        for chunk in self.chunks.iter_mut() {
            chunk.clear_viewable_packets();
            std::mem::swap(&mut chunk.pending_solid_entity_aabbs, &mut chunk.solid_entity_aabbs);
            std::mem::swap(&mut chunk.pending_soft_entity_aabbs, &mut chunk.soft_entity_aabbs);
            chunk.pending_solid_entity_aabbs.clear();
            chunk.pending_soft_entity_aabbs.clear();
        }

        ServiceTickAction::None
    }

}

impl <W: WorldExtension + 'static> World<W> {
    pub fn start<F: 'static + Send + FnOnce() -> Box<World<W>>>(world: F) -> WorldController<W> {
        let terminate = Arc::new(AtomicBool::new(false));
        let terminate2 = terminate.clone();

        let terminated = Arc::new(AtomicBool::new(false));
        let terminated2 = terminated.clone();

        let service = move || {
            let mut world = world();
            world.terminate = Some(terminate2);
            world.terminated = Some(terminated2);
            world
        };

        WorldController {
            terminate,
            terminated,
            join: graphite_network::NetworkHandler::start(service).unwrap(),
        }
    }

    pub fn new(extension: W, mut chunk_list: ChunkList) -> Self {
        assert!(W::VIEW_DISTANCE >= 2 && W::VIEW_DISTANCE <= 32);

        let chunks_x = chunk_list.size_x as i32;
        let chunks_z = chunk_list.size_z as i32;

        let mut chunks = Vec::with_capacity((chunks_x * chunks_z) as usize);

        for index in 0..chunks_x*chunks_z {
            chunks.push(Chunk::new(std::mem::take(&mut chunk_list.chunks[index as usize])));
        }

        let mut registries = Registries::default();
        registries.dimension_type.dimensions[0].1.height = chunk_list.size_y as i32 * 16;

        Self {
            inbound_players: Vec::new(),
            outbound_players: Vec::new(),
            players: Slab::new(),
            player_generation: 0,

            registries,

            entities_for_removal: Vec::new(),

            terminate: None,
            terminated: None,

            entities: hecs::World::new(),
            pending_entities: Vec::new(),

            chunks: chunks.into_boxed_slice(),
            chunks_x,
            chunks_z,
            empty_chunk: Chunk::new_empty(chunk_list.size_y),
            
            remote_entities: Slab::new(),
            remote_entity_generation: 0,
            remote_entity_by_network_id: HashMap::default(),

            pathfinding_cache: PathfindingCacheHolder::new(),

            extension
        }
    }

    pub fn player<P: PlayerExtension + 'static>(&self, player_id: PlayerId) -> Option<&Player<P>> {
        unsafe { self.players.get(player_id.slab_index)?.get().as_mut() }.unwrap().downcast_ref::<Player<P>>()
            .filter(|player| player.player_id == player_id)
    }

    pub fn player_mut<P: PlayerExtension + 'static>(&mut self, player_id: PlayerId) -> Option<&mut Player<P>> {
        unsafe { self.players.get_mut(player_id.slab_index)?.get().as_mut() }.unwrap().downcast_mut::<Player<P>>()
            .filter(|player| player.player_id == player_id)
    }

    pub fn write_spawn_entities_and_players<P: PlayerExtension>(&mut self, x: i32, z: i32, player: &mut Player<P>) {
        if x < 0 || z < 0 || x >= self.chunks_x || z >= self.chunks_z {
            return;
        }

        let chunk = &mut self.chunks[(x + z * self.chunks_x) as usize];
        chunk.write_spawn_entities_and_players(&self.entities, &self.remote_entities, player);
    }

    pub fn write_despawn_entities_and_players<P: PlayerExtension>(&mut self, x: i32, z: i32, despawn_list: &mut Vec<i32>, player: &mut Player<P>) {
        if x < 0 || z < 0 || x >= self.chunks_x || z >= self.chunks_z {
            return;
        }

        let chunk = &mut self.chunks[(x + z * self.chunks_x) as usize];
        chunk.write_despawn_entities_and_players(&self.entities, &self.remote_entities, despawn_list, player);
    }

    pub fn set_block(&mut self, x: i32, y: i32, z: i32, block: u16) -> u16 {
        if let Some(chunk) = self.get_chunk_mut(x >> 4, z >> 4) {
            chunk.set_block(x, y, z, block)
        } else {
            0
        }
    }

    pub(crate) fn get_block_for_client(&self, x: i32, y: i32, z: i32) -> Option<u16> {
        self.get_chunk(x >> 4, z >> 4)
            .and_then(|chunk| chunk.get_block_for_client(x, y, z))
    }

    pub fn entity_world(&self) -> &hecs::World {
        &self.entities
    }

    pub fn get_entity(&self, entity: hecs::Entity) -> Option<hecs::EntityRef<'_>> {
        self.entities.entity(entity).ok()
    }

    pub fn get_entity_query<Q: hecs::Query>(&self) -> QueryBorrow<'_, Q> {
        self.entities.query::<Q>()
    }

    pub fn query_one<Q: hecs::Query>(&self, entity: hecs::Entity) -> SingleQuery<'_, Q> {
        SingleQuery {
            query: self.entities.query_one::<Q>(entity).ok()
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

    pub fn spawn_player<P: PlayerExtension<World = W> + 'static>(&mut self, position: DVec3, yaw: f32, pitch: f32,
            connection: Rc<RefCell<Connection>>, profile: GameProfile<'static>, inventory: P::InventoryContainer, extension: P) -> &mut Player<P> {
        let self_ptr = self.into();

        // Create and insert player
        let vacant = self.players.vacant_entry();
        self.player_generation += 1;
        let player_id = PlayerId {
            slab_index: vacant.key(),
            generation: self.player_generation
        };

        let mut player = Player::new(self_ptr, player_id, position, connection, profile.clone(), inventory, extension);
        player.yaw = yaw;
        player.pitch = pitch;
        let player_cell = Rc::new(UnsafeCell::new(player));

        vacant.insert(player_cell.clone());

        // Set connection handler
        let player_ref = unsafe { player_cell.get().as_mut() }.unwrap();
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
            dimension_names: vec!["graphite:world"],
            max_players: 69420,
            view_distance: W::VIEW_DISTANCE as i32,
            simulation_distance: W::VIEW_DISTANCE as i32,
            reduced_debug_info: false,
            enable_respawn_screen: false,
            do_limited_crafting: false,
            dimension_type: 0,
            dimension_name: "graphite:world",
            hashed_seed: 0,
            gamemode: 0,
            previous_gamemode: -1,
            is_debug: false,
            is_flat: false,
            death_location: None,
            portal_cooldown: 0,
            sea_level: 70,
            enforces_secure_chat: true
        };
        player_ref.send_packet(&join_game);

        // send respawn
        let respawn_packet = play::clientbound::Respawn {
            dimension_type: 0,
            dimension_name: "graphite:world",
            hashed_seed: 0,
            gamemode: 0,
            previous_gamemode: -1,
            is_debug: false,
            is_flat: false,
            death_location: None,
            portal_cooldown: 0,
            sea_level: 70,
            data_to_keep: 0
        };
        player_ref.send_packet(&respawn_packet);

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
                        Tag {
                            name: "minecraft:bundles",
                            entries: vec![Item::Bundle as u16, Item::PoppedChorusFruit as u16]
                        },
                    ]
                },
                TagRegistry {
                    tag_type: "minecraft:block",
                    values: vec![
                        Tag {
                            name: "minecraft:climbable",
                            entries: vec![Block::CaveVines as u16, Block::CaveVinesPlant as u16, Block::Ladder as u16, Block::Vine as u16]
                        }
                    ]
                }
            ],
        };
        player_ref.send_packet(&tags);

        // send brand
        player_ref.send_packet(&play::clientbound::CustomPayload {
            channel: "minecraft:brand",
            data: b"\x08Graphite",
        });

        // send teleport
        player_ref.teleport_full(yaw, pitch, position, DVec3::ZERO, EnumSet::empty());

        let new_player_info_update = play::clientbound::PlayerInfoUpdate {
            actions: PlayerInfoAction::AddPlayer | PlayerInfoAction::UpdateListed | PlayerInfoAction::UpdateGameMode,
            entries: vec![
                PlayerInfoEntry {
                    profile,
                    listed: true,
                    latency: 0,
                    gamemode: 0,
                    display_name: None
                }
            ],
        };

        // Write player info (including self)
        let mut existing_player_info = Vec::with_capacity(self.players.len() - 1);
        for (index, player) in &self.players {
            let player = unsafe { player.get().as_mut() }.unwrap();

            new_player_info_update.write_packet(player.get_packet_buffer());

            if index != player_id.slab_index {
                existing_player_info.push(PlayerInfoEntry {
                    profile: player.get_game_profile(),
                    listed: true,
                    latency: 0,
                    gamemode: 0,
                    display_name: None,
                });
            }
        }

        player_ref.send_packet(&play::clientbound::PlayerInfoUpdate {
            actions: PlayerInfoAction::AddPlayer | PlayerInfoAction::UpdateListed | PlayerInfoAction::UpdateGameMode,
            entries: existing_player_info,
        });

        player_ref.send_packet(&play::clientbound::SetTime {
            game_time: 0,
            day_time: 6000,
            tick_day_time: false
        });

        // send StartWaitingForLevelChunks game event
        player_ref.send_packet(&play::clientbound::GameEvent {
            event_type: GameEventType::StartWaitingForLevelChunks,
            param: 0.0,
        });

        player_ref.send_packet(&play::clientbound::SetChunkCacheCenter {
            chunk_x,
            chunk_z,
        });

        // write initial chunks
        let view_distance = W::VIEW_DISTANCE as i32;
        for x in (chunk_x-view_distance) .. (chunk_x+view_distance+1) {
            for z in (chunk_z-view_distance) .. (chunk_z+view_distance+1) {
                if x >= 0 && z >= 0 && x < self.chunks_x && z < self.chunks_z {
                    let chunk = &mut self.chunks[(x + z * self.chunks_x) as usize];
                    chunk.write(&mut player_ref.packet_buffer, x, z);
                } else {
                    self.empty_chunk.write(&mut player_ref.packet_buffer, x, z);
                }
            }
        }

        // resend teleport after chunks
        player_ref.teleport_full(yaw, pitch, position, DVec3::ZERO, EnumSet::empty());

        // write initial entities
        let view_distance = W::ENTITY_VIEW_DISTANCE as i32;
        for x in (chunk_x-view_distance).max(0) .. (chunk_x+view_distance+1).min(self.chunks_x) {
            for z in (chunk_z-view_distance).max(0) .. (chunk_z+view_distance+1).min(self.chunks_z) {
                let chunk = &mut self.chunks[(x + z * self.chunks_x) as usize];
                
                // Write entity spawn packets
                chunk.write_spawn_entities_and_players(&self.entities, &self.remote_entities, player_ref);
            }
        }

        player_ref.update_commands();

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

    pub fn nearby_players<P: PlayerExtension>(&self, position: DVec3, distance: f64) -> NearbyPlayerIterator<'_, P> {
        NearbyPlayerIterator {
            chunks: self.nearby_chunks(position.xz(), distance),
            chunk_players: None,
            position,
            distance_sq: distance * distance,
            phantom: std::marker::PhantomData,
        }
    }

    pub fn nearby_players_mut<P: PlayerExtension>(&mut self, position: DVec3, distance: f64) -> NearbyPlayerIterMut<'_, P> {
        NearbyPlayerIterMut {
            chunks: self.nearby_chunks_mut(position.xz(), distance),
            chunk_players: None,
            position,
            distance_sq: distance * distance,
            phantom: std::marker::PhantomData,
        }
    }

    pub fn query_nearby<Q: hecs::Query>(&self, position: DVec3, distance: f64, query: &mut hecs::PreparedQuery<(&mut EntityBase, Q)>,
            mut function: impl FnMut(&World<W>, &mut EntityBase, Q::Item<'_>)) {
        let mut borrow = query.query(&self.entities);
        let mut view = borrow.view();
        let distance_sq = distance * distance;
        for chunk in self.nearby_chunks(position.xz(), distance) {
            for (_, &entity) in &chunk.entities {
                if let Some((base, item)) = view.get_mut(entity) {
                    if base.position.distance_squared(position) <= distance_sq {
                        (function)(self, base, item);
                    }
                }
            }
        }
    }

    pub fn query_nearby_with_control_flow<Q: hecs::Query, R>(&self, position: DVec3, distance: f64, query: &mut hecs::PreparedQuery<(&mut EntityBase, Q)>,
            mut function: impl FnMut(&World<W>, &mut EntityBase, Q::Item<'_>) -> ControlFlow<R>) -> Option<R> {
        let mut borrow = query.query(&self.entities);
        let mut view = borrow.view();
        let distance_sq = distance * distance;
        for chunk in self.nearby_chunks(position.xz(), distance) {
            for (_, &entity) in &chunk.entities {
                if let Some((base, item)) = view.get_mut(entity) {
                    if base.position.distance_squared(position) <= distance_sq {
                        let result = (function)(self, base, item);
                        if let ControlFlow::Break(ret) = result {
                            return Some(ret);
                        }
                    }
                }
            }
        }
        None
    }

    pub fn spawn_particle(&mut self, x: f64, y: f64, z: f64, particle: Particle) {
        let chunk_x = (x.floor() as i32) >> 4;
        let chunk_z = (z.floor() as i32) >> 4;

        if let Some(chunk) = self.get_chunk_mut(chunk_x, chunk_z) {
            LevelParticles {
                override_limiter: true,
                always_show: false,
                x,
                y,
                z,
                offset_x: 0.0,
                offset_y: 0.0,
                offset_z: 0.0,
                max_speed: 0.0,
                particle_count: 0,
                particle
            }.write_packet(&mut chunk.chunk_viewable);
        }
    }

    pub fn spawn_particles(&mut self, x: f64, y: f64, z: f64, offset_x: f32, offset_y: f32,
            offset_z: f32, max_speed: f32, particle_count: i32, particle: Particle) {
        let chunk_x = (x.floor() as i32) >> 4;
        let chunk_z = (z.floor() as i32) >> 4;

        if let Some(chunk) = self.get_chunk_mut(chunk_x, chunk_z) {
            LevelParticles {
                override_limiter: true,
                always_show: false,
                x,
                y,
                z,
                offset_x,
                offset_y,
                offset_z,
                max_speed,
                particle_count,
                particle
            }.write_packet(&mut chunk.chunk_viewable);
        }
    }

    pub fn spawn_debug_particle(&mut self, x: f64, y: f64, z: f64) {
        self.spawn_particle(x, y, z, Particle::Composter)
    }

    pub fn raycast<F, T>(&self, from: DVec3, to: DVec3, mut func: F) -> Option<T>
    where
        F: FnMut(i32, i32, i32, u16) -> ControlFlow<T>
    {
        let delta = to - from;
        let direction = delta.try_normalize()?;

        let mut map_x = from.x.floor() as i32;
        let mut map_y = from.y.floor() as i32;
        let mut map_z = from.z.floor() as i32;

        let to_map_x = to.x.floor() as i32;
        let to_map_y = to.y.floor() as i32;
        let to_map_z = to.z.floor() as i32;

        let block = self.get_block(map_x, map_y, map_z)?;
        if let ControlFlow::Break(v) = (func)(map_x, map_y, map_z, block) {
            return Some(v);
        }

        if map_x == to_map_x && map_y == to_map_y && map_z == to_map_z {
            return None;
        }

        let delta_dist_x = (1.0 / direction.x).abs();
        let delta_dist_y = (1.0 / direction.y).abs();
        let delta_dist_z = (1.0 / direction.z).abs();

        let (step_x, mut side_dist_x) = if direction.x > 0.0 {
            (1, 1.0 - from.x + map_x as f64)
        } else {
            (-1, from.x - map_x as f64)
        };
        let (step_y, mut side_dist_y) = if direction.y > 0.0 {
            (1, 1.0 - from.y + map_y as f64)
        } else {
            (-1, from.y - map_y as f64)
        };
        let (step_z, mut side_dist_z) = if direction.z > 0.0 {
            (1, 1.0 - from.z + map_z as f64)
        } else {
            (-1, from.z - map_z as f64)
        };

        side_dist_x *= delta_dist_x;
        side_dist_y *= delta_dist_y;
        side_dist_z *= delta_dist_z;

        if side_dist_x.is_nan() {
            side_dist_x = f64::INFINITY;
        }
        if side_dist_y.is_nan() {
            side_dist_y = f64::INFINITY;
        }
        if side_dist_z.is_nan() {
            side_dist_z = f64::INFINITY;
        }

        loop {
            if side_dist_z < side_dist_x && side_dist_z < side_dist_y {
                side_dist_z += delta_dist_z;
                map_z += step_z;
                if map_z*step_z > to_map_z*step_z {
                    return None;
                }
            } else if side_dist_x < side_dist_y {
                side_dist_x += delta_dist_x;
                map_x += step_x;
                if map_x*step_x > to_map_x*step_x {
                    return None;
                }
            } else {
                side_dist_y += delta_dist_y;
                map_y += step_y;
                if map_y*step_y > to_map_y*step_y {
                    return None;
                }
            }

            let block = self.get_block(map_x, map_y, map_z)?;
            if let ControlFlow::Break(v) = (func)(map_x, map_y, map_z, block) {
                return Some(v);
            }
        }
    }

    fn process_entity_pending(&mut self) {
        // Add new pending entities
        let world_ptr = self.into();
        for (entity_id, mut new) in self.pending_entities.drain(..) {
            self.entities.spawn_at(entity_id, new.build());
            let entity_ref = self.entities.entity(entity_id).unwrap();

            let mut base = entity_ref.get::<&mut EntityBase>().unwrap();

            // Register pending remote entities
            for mut remote_entity in std::mem::take(&mut base.pending_remote_entities) {
                let vacant = self.remote_entities.vacant_entry();
                let index = vacant.key();

                // Create and set RemoteEntityId
                self.remote_entity_generation += 1;
                let id = RemoteEntityId {
                    slab_index: index,
                    generation: self.remote_entity_generation
                };
                remote_entity.id = Some(id);
                remote_entity.ecs_id = entity_id;

                // Put entity
                let network_id = remote_entity.network_id();
                vacant.insert(remote_entity);
                self.remote_entity_by_network_id.insert(network_id, index);

                base.remote_entities.push(id);
            }

            base.add_to_world(world_ptr, &self.remote_entities, entity_ref);
        }

        // Remove entities awaiting removal
        for remove in self.entities_for_removal.drain(..) {
            let entity_ref = self.entities.entity(remove).unwrap();
            let mut entity_base = entity_ref.get::<&mut EntityBase>().unwrap();

            debug_assert!(entity_base.pending_remove);

            entity_base.remove_from_world(&self.remote_entities, entity_ref);

            // Remove remote entities
            for &id in &entity_base.remote_entities {
                let remote_entity = self.remote_entities.remove(id.slab_index);
                debug_assert_eq!(remote_entity.id, Some(id));
                let removed_index = self.remote_entity_by_network_id.remove(&remote_entity.network_id());
                debug_assert_eq!(removed_index, Some(id.slab_index));
            }

            // Remove entity from hecs::World
            drop(entity_base);
            let despawn = self.entities.despawn(remove);
            debug_assert!(despawn.is_ok());
        }
    }

    fn handle_inbound_players(&mut self) {
        // Can't use retain here since on_player_join requires a reference to the world
        let mut index = 0;
        loop {
            let Some(player) = self.inbound_players.get(index) else {
                break;
            };

            let player_ref = unsafe { player.get().as_mut() }.unwrap();
            if player_ref.tick_should_remove() {
                player_ref.disconnect(None);
                drop(self.inbound_players.swap_remove(index)); // drop this after or otherwise the player_ref is invalid
            } else if let Some((data, connection)) = player_ref.try_end() {
                drop(self.inbound_players.swap_remove(index)); // drop this before since it's possible for on_player_join to modify inbound_players
                W::on_player_join(self, data, connection);
            } else {
                index += 1;
            }
        }
    }

    fn handle_outbound_players(&mut self) {
        self.outbound_players.retain(|player_rc| {
            let player = unsafe { player_rc.get().as_mut() }.unwrap();

            if player.tick_should_remove() {
                player.disconnect(None);
                false
            } else {
                true
            }
        });
    }

    fn handle_player_updates(&mut self, uuids: &mut Vec<u128>, entity_ids: &mut Vec<i32>, do_tick: bool) {
        let outbound_players = &mut self.outbound_players;

        self.players.retain(|_, player_rc| {
            let player = unsafe { player_rc.get().as_mut() }.unwrap();

            if Self::do_disconnect_or_transfer(outbound_players, player, uuids, entity_ids) {
                assert!(Rc::strong_count(player_rc) == 1, "Reference to player was not dropped, this would have led to a memory leak");
                return false;
            }

            if do_tick {
                player.tick();

                if Self::do_disconnect_or_transfer(outbound_players, player, uuids, entity_ids) {
                    assert!(Rc::strong_count(player_rc) == 1, "Reference to player was not dropped, this would have led to a memory leak");
                    return false;
                }
            }

            true
        });
    }

    fn do_disconnect_or_transfer(outbound_players: &mut Vec<Rc<UnsafeCell<OutboundPlayer>>>,
            player: &mut dyn GenericPlayer, uuids: &mut Vec<u128>, entity_ids: &mut Vec<i32>) -> bool {
        if !player.is_still_connected() {
            uuids.push(player.get_uuid());
            entity_ids.push(player.get_entity_id());

            if let Some((connection, mut buffer, transfer)) = player.take_transfer() {
                play::clientbound::StartConfiguration.write_packet(&mut buffer);
                
                let mut connection_ref = connection.borrow_mut();
                connection_ref.send(&mut buffer);
                connection_ref.disconnect_handler();
                drop(connection_ref);
    
                let outbound_player = OutboundPlayer::new(connection, buffer, transfer);
                outbound_players.push(outbound_player);
            }

            player.clear_references();
    
            true
        } else {
            false
        }
    }
}