use std::{borrow::Cow, cell::RefCell, ops::ControlFlow, ptr::NonNull};

use glam::{DVec2, DVec3, Vec3Swizzles};
use graphite_binary::slice_serialization::SliceSerializable;
use graphite_mc_constants::{block::{BlockAttributes, BlockFlag}, entity::{EntityAndMetadata, InteractionMetadata}, particle::Particle, types::{EntityAnimation, SoundSource}};
use graphite_mc_protocol::{play::clientbound::{AnimateEntity, BundledPacketBuffer, EntityEvent, LevelParticles, RemoveEntities, SoundEntity}, types::SoundType, IdentifiedPacket};
use graphite_network::PacketBuffer;
use rand::RngCore;
use slab::Slab;

use crate::{types::AABB, world::{chunk::{Chunk, ChunkEntityRef}, chunk_view_diff::{self, ChunkDiffStatus}, GenericWorld, RemoteEntityId, World, WorldExtension}};

use super::{entity_view::EntityView, remote_entity::RemoteEntity};

#[derive(Clone)]
pub struct Collision {
    pub aabb: AABB,
    pub step_height: f32,
    pub only_step_if_moving_forwards: bool,
    pub calculate_falling: bool,
    falling_distance: f32,
    pub accumulated_fallen_distance: f32,
    pub last_collision_mask: u8

}

impl Collision {
    pub fn new(aabb: AABB, step_height: f32, only_step_if_moving_forwards: bool, calculate_falling: bool) -> Self {
        Self {
            aabb,
            step_height,
            only_step_if_moving_forwards,
            calculate_falling,
            falling_distance: 0.0,
            accumulated_fallen_distance: 0.0,
            last_collision_mask: 0
        }
    }
}

thread_local! {
    static SCRATCH_BUFFERS: RefCell<(PacketBuffer, PacketBuffer, Vec<i32>)> = RefCell::new((PacketBuffer::new(), PacketBuffer::new(), Vec::new()));
}

pub struct EntityBase {
    world: Option<NonNull<dyn GenericWorld>>,
    pub self_id: hecs::Entity,
    pub(crate) pending_remove: bool,
    pub(crate) remove_pickup_target: Option<i32>,

    pub position: DVec3,
    pub velocity: DVec3,
    pub rotation: DVec2,
    pub on_ground: bool,

    pub after_added: fn(entity: hecs::EntityRef, base: &mut EntityBase),
    pub before_removed: fn(entity: hecs::EntityRef, base: &mut EntityBase),

    pub(crate) pending_remote_entities: Vec<RemoteEntity>,
    pub(crate) remote_entities: Vec<RemoteEntityId>,

    entity_view_distance: u8,
    last_chunk_x: i32,
    last_chunk_z: i32,
    max_chunk_x: i32,
    max_chunk_z: i32,
    pub(crate) chunk_ref: Option<ChunkEntityRef>,
}

unsafe impl Send for EntityBase {}
unsafe impl Sync for EntityBase {}

impl Default for EntityBase {
    fn default() -> Self {
        Self {
            world: None,
            self_id: hecs::Entity::DANGLING,
            pending_remove: false,
            remove_pickup_target: None,

            position: DVec3::ZERO,
            velocity: DVec3::ZERO,
            rotation: DVec2::ZERO,
            on_ground: false,

            after_added: |_, _| {},
            before_removed: |_, _| {},

            pending_remote_entities: Vec::new(),
            remote_entities: Vec::new(),

            entity_view_distance: 0,
            last_chunk_x: 0,
            last_chunk_z: 0,
            max_chunk_x: 0,
            max_chunk_z: 0,
            chunk_ref: None,
        }
    }
}

impl EntityBase {
    pub fn new(position: DVec3, velocity: DVec3, rotation: DVec2) -> Self {
        Self {
            position,
            velocity,
            rotation,
            
            last_chunk_x: (position.x.floor() as i32) >> 4,
            last_chunk_z: (position.z.floor() as i32) >> 4,

            ..Default::default()
        }
    }

    pub(crate) fn add_to_world<W: WorldExtension>(
        &mut self,
        world: NonNull<World<W>>,
        remote_entities: &Slab<RemoteEntity>,
        entity_ref: hecs::EntityRef
    ) {
        assert!(self.world.is_none());

        self.world = Some(world);
        self.self_id = entity_ref.entity();
        self.entity_view_distance = W::ENTITY_VIEW_DISTANCE;

        self.max_chunk_x = self.generic_world().chunks_x() as i32 - 1;
        self.max_chunk_z = self.generic_world().chunks_z() as i32 - 1;
        self.last_chunk_x = ((self.position.x.floor() as i32) >> 4).clamp(0, self.max_chunk_x);
        self.last_chunk_z = ((self.position.z.floor() as i32) >> 4).clamp(0, self.max_chunk_z);

        if let Some(chunk) = self.get_last_chunk_mut() {
            self.chunk_ref = Some(chunk.insert_entity(entity_ref.entity()));
        }

        // Write spawn packets
        let mut spawned = false;
        if let Some(view) = entity_ref.get::<&EntityView>() {
            if let Some(spawn) = view.spawn {
                self.write_viewable_immediate(|base, buffer| {
                    let mut bundle = BundledPacketBuffer::new(buffer);

                    (spawn)(entity_ref, base, &*view, &mut *bundle);
                    for remote_entity in &base.remote_entities {
                        remote_entities[remote_entity.slab_index].spawn(&mut *bundle, base.position, base.rotation.y, base.rotation.x);
                    }
                });
                spawned = true;
            }
        }

        if !spawned && !self.remote_entities.is_empty() {
            self.write_viewable_immediate(|base, buffer| {
                let mut bundle = BundledPacketBuffer::new(buffer);

                for remote_entity in &base.remote_entities {
                    remote_entities[remote_entity.slab_index].spawn(&mut *bundle, base.position, base.rotation.y, base.rotation.x);
                }
            });
        }

        // Invoke added callback
        (self.after_added)(entity_ref, self);
    }

    pub(crate) fn remove_from_world(&mut self, remote_entities: &Slab<RemoteEntity>, entity_ref: hecs::EntityRef) {
        // Invoke removal callback
        (self.before_removed)(entity_ref, self);

        // Send despawn packets
        let mut despawned = false;
        if let Some(view) = entity_ref.get::<&EntityView>() {
            if view.despawn.is_some() || !view.entity_ids.is_empty() {
                despawned = true;
                let chunk_x = self.last_chunk_x;
                let chunk_z = self.last_chunk_z;
    
                SCRATCH_BUFFERS.with(|buffers| {
                    let (_, despawn_buffer, despawn_vec) = &mut *buffers.borrow_mut();
    
                    let mut written_packets = false;
    
                    // Write despawn packets
                    let view_distance = self.entity_view_distance as i32;
                    for x in (chunk_x-view_distance) .. (chunk_x+view_distance+1) {
                        for z in (chunk_z-view_distance) .. (chunk_z+view_distance+1) {
                            let Some(mut chunk) = self.get_chunk_mut(x, z) else {
                                continue;
                            };
    
                            if chunk.has_players() {
                                if !written_packets {
                                    let mut bundle = BundledPacketBuffer::new(despawn_buffer);

                                    for remote_entity in &self.remote_entities {
                                        remote_entities[remote_entity.slab_index].despawn(despawn_vec, &mut *bundle);
                                    }

                                    despawn_vec.extend(&view.entity_ids);
                                    if let Some(despawn) = view.despawn {
                                        (despawn)(entity_ref, self, &*view, despawn_vec, &mut *bundle);
                                    }
                                    if !despawn_vec.is_empty() {
                                        if let Some(target) = self.remove_pickup_target {
                                            for to_remove in despawn_vec.iter() {
                                                bundle.write(&graphite_mc_protocol::play::clientbound::TakeItemEntity {
                                                    item_id: *to_remove,
                                                    player_id: target,
                                                    amount: 64
                                                });
                                            }
                                        } else {
                                            bundle.write(&graphite_mc_protocol::play::clientbound::RemoveEntities {
                                                entities: Cow::Borrowed(&*despawn_vec),
                                            });
                                        }
                                    }

                                    chunk = self.get_chunk_mut(x, z).unwrap(); // reborrow
                                    written_packets = true;
                                }
    
                                chunk.write_immediately_to_players(despawn_buffer.peek_written());
                            }
                        }
                    }
    
                    despawn_vec.clear();
                    despawn_buffer.clear();
                });
            }
        }

        if !despawned && !self.remote_entities.is_empty() {
            self.write_viewable_immediate(|base, buffer| {
                let mut ids = Vec::new();
                for remote_entity in &base.remote_entities {
                    remote_entities[remote_entity.slab_index].despawn(&mut ids, buffer);
                }
                if !ids.is_empty() {
                    graphite_mc_protocol::play::clientbound::RemoveEntities {
                        entities: Cow::Owned(ids),
                    }.write_packet(buffer);
                }
            });
        }

        // Remove from chunk
        let chunk_ref = self.chunk_ref.take();
        if let Some(chunk) = self.get_last_chunk_mut() {
            chunk.remove_entity(chunk_ref.unwrap());
        }

        self.world = None;
    }

    pub fn is_valid(&self) -> bool {
        !self.pending_remove && self.self_id != hecs::Entity::DANGLING && self.world.is_some()
    }

    pub fn remove(&mut self) {
        if !self.pending_remove {
            self.pending_remove = true;
            let id = self.self_id;
            self.generic_world_mut().mark_for_removal(id);
        }
    }

    pub fn remove_with_pickup(&mut self, target: i32) {
        self.remove();
        self.remove_pickup_target = Some(target);
    }

    pub fn get_primary_entity_id(&mut self) -> Option<i32> {
        if let Some(remote_id) = self.remote_entities.first() {
            let remote_entity = self.generic_world().get_remote_entity(*remote_id);
            if let Some(remote_entity) = remote_entity {
                return Some(remote_entity.network_id());
            }
        }

        // Note: We skip borrow checking with query_one_mut. This is probably bad, but is needed in order to not panic.
        unsafe {
            let self_id = self.self_id;
            let entities = self.generic_world_mut().entity_world_mut();
            let view = entities.query_one_mut::<&EntityView>(self_id).ok()?;
            view.primary_entity_id
        }
    }

    pub fn play_animation(&mut self, animation: EntityAnimation) {
        let Some(entity_id) = self.get_primary_entity_id() else {
            return;
        };

        self.add_viewable_packet(&AnimateEntity {
            entity_id,
            animation,
        });
    }

    pub fn send_entity_event(&mut self, status: u8) {
        let Some(entity_id) = self.get_primary_entity_id() else {
            return;
        };

        self.add_viewable_packet(&EntityEvent {
            entity_id,
            status,
        });
    }
    
    pub fn remote_entities(&self) -> &Vec<RemoteEntityId> {
        &self.remote_entities
    }

    pub fn emit_sound<'a>(&mut self, sound: SoundType<'a>, source: SoundSource) {
        self.emit_sound_with_levels(sound, source, 1.0, 1.0)
    }

    pub fn emit_sound_with_levels<'a>(&mut self, sound: SoundType<'a>, source: SoundSource, volume: f32, pitch: f32) {
        let Some(entity_id) = self.get_primary_entity_id() else {
            return;
        };

        self.add_viewable_packet(&SoundEntity {
            sound,
            source,
            entity_id,
            volume,
            pitch,
            seed: rand::thread_rng().next_u64(),
        });
    }

    pub fn emit_particle(&mut self, x: f64, y: f64, z: f64, particle: Particle) {
        self.add_viewable_packet(&LevelParticles {
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
        });
    }

    pub fn emit_particles(&mut self, x: f64, y: f64, z: f64, offset_x: f32, offset_y: f32,
            offset_z: f32, max_speed: f32, particle_count: i32, particle: Particle) {
        self.add_viewable_packet(&LevelParticles {
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
        });
    }

    pub fn world<W: WorldExtension>(&self) -> Option<&World<W>> {
        unsafe {
            self.world.and_then(|w| w.as_ref().downcast_ref())
        }
    }

    pub fn world_mut<W: WorldExtension>(&mut self) -> Option<&mut World<W>> {
        unsafe {
            self.world.and_then(|mut w| w.as_mut().downcast_mut())
        }
    }

    pub fn generic_world(&self) -> &dyn GenericWorld {
        unsafe {
            self.world.unwrap().as_ref()
        }
    }

    pub fn generic_world_mut(&mut self) -> &mut dyn GenericWorld {
        unsafe {
            self.world.unwrap().as_mut()
        }
    }

    pub fn add_interaction_entity(&mut self, width: f32, height: f32, response: bool) {
        let mut metadata = InteractionMetadata::default();
        metadata.set_width(width);
        metadata.set_height(height);
        metadata.set_response(response);
        self.add_remote_entity(RemoteEntity::new(EntityAndMetadata::Interaction(metadata)));
    }

    pub fn add_remote_entity(&mut self, remote_entity: RemoteEntity) {
        if self.world.is_some() && self.self_id != hecs::Entity::DANGLING {
            let _ = self.spawn_remote_entity(remote_entity);
        } else {
            self.pending_remote_entities.push(remote_entity);
        }
    }

    #[must_use = "if not using returned id, use add_remote_entity() instead"]
    pub fn spawn_remote_entity(&mut self, mut remote_entity: RemoteEntity) -> RemoteEntityId {
        if self.world.is_none() || self.self_id == hecs::Entity::DANGLING {
            panic!("Can't spawn remote entity since entity hasn't been added to world yet. Use add_remote_entity instead.");
        }

        remote_entity.ecs_id = self.self_id;

        self.write_viewable_immediate(|base, buffer| {
            remote_entity.spawn(buffer, base.position, base.rotation.y, base.rotation.x);
        });

        let id = self.generic_world_mut().register_remote_entity(remote_entity);

        self.remote_entities.push(id);

        id
    }

    pub fn remove_remote_entity(&mut self, remote_entity_id: RemoteEntityId) -> Option<RemoteEntity> {
        let Some(index) = self.remote_entities.iter().position(|&id| id == remote_entity_id) else {
            return None;
        };

        self.remote_entities.remove(index);

        let remote_entity = self.generic_world_mut().deregister_remote_entity(remote_entity_id)?;

        self.write_viewable_immediate(|_, buffer| {
            let mut despawn_vec = Vec::new();
            remote_entity.despawn(&mut despawn_vec, buffer);
            if !despawn_vec.is_empty() {
                RemoveEntities {
                    entities: Cow::Owned(despawn_vec),
                }.write_packet(buffer);
            }
        });

        Some(remote_entity)
    }
    
    pub fn add_viewable_packet<'r, 'd: 'r, I: std::fmt::Debug, T>(&mut self, packet: &'r T)
    where
        T: SliceSerializable<'r, 'd, T> + IdentifiedPacket<I> + 'd,
    {
        if let Some(chunk) = self.get_last_chunk_mut() {
            chunk.add_entity_viewable_packet(packet);
        }
    }

    pub fn write_viewable(&mut self, lambda: impl FnOnce(&mut PacketBuffer)) {
        if let Some(chunk) = self.get_last_chunk_mut() {
            chunk.write_viewable(lambda);
        }
    }

    pub fn write_viewable_immediate(&mut self, mut lambda: impl FnMut(&mut EntityBase, &mut PacketBuffer)) {
        let chunk_x = self.last_chunk_x;
        let chunk_z = self.last_chunk_z;

        SCRATCH_BUFFERS.with(|buffers| {
            let (buffer, _, _) = &mut *buffers.borrow_mut();

            let mut written_packets = false;

            let view_distance = self.entity_view_distance as i32;
            for x in (chunk_x-view_distance) .. (chunk_x+view_distance+1) {
                for z in (chunk_z-view_distance) .. (chunk_z+view_distance+1) {
                    if let Some(mut chunk) = self.get_chunk_mut(x, z) {
                        if chunk.has_players() {
                            if !written_packets {
                                lambda(self, buffer);
                                chunk = self.get_chunk_mut(x, z).unwrap(); // reborrow
                                written_packets = true;
                            }

                            chunk.write_immediately_to_players(buffer.peek_written());
                        }
                    }
                }
            }

            buffer.clear();
        });
    }

    pub fn get_chunk_mut(&mut self, chunk_x: i32, chunk_z: i32) -> Option<&mut Chunk> {
        unsafe {
            self.world.as_mut().and_then(|w| w.as_mut().get_chunk_mut(chunk_x, chunk_z))
        }
    }

    pub fn get_chunk(&self, chunk_x: i32, chunk_z: i32) -> Option<&Chunk> {
        unsafe {
            self.world.as_ref().and_then(|w| w.as_ref().get_chunk(chunk_x, chunk_z))
        }
    }

    pub fn last_chunk_coords(&self) -> (i32, i32) {
        (self.last_chunk_x, self.last_chunk_z)
    }

    pub fn get_last_chunk_mut(&mut self) -> Option<&mut Chunk> {
        self.get_chunk_mut(self.last_chunk_x, self.last_chunk_z)
    }

    pub(crate) fn update_without_collision(&mut self) {
        if self.velocity.abs().max_element() < 0.001 {
            self.velocity = DVec3::ZERO;
        } else {
            self.position += self.velocity;
            self.on_ground = false;
        }
    }

    pub(crate) fn update_with_collision(&mut self, collision: &mut Collision) {
        collision.accumulated_fallen_distance = 0.0;
        collision.last_collision_mask = 0;

        if self.velocity.abs().max_element() < 0.001 {
            self.velocity = DVec3::ZERO;
            return;
        }

        let aabb = collision.aabb.translate(self.position);
        let velocity = self.velocity;
        let (moved, hit_x, hit_y, hit_z) = self.generic_world().shape_cast(aabb, velocity);
        collision.last_collision_mask = hit_x as u8 | ((hit_y as u8) << 1) | ((hit_z as u8) << 2);

        if collision.calculate_falling && !self.on_ground {
            let reset_fall_distance = self.generic_world().raycast_generic_bool(self.position, self.position + moved, 
                Box::new(|_, _, _, block_state| {
                if block_state != 0 {
                    let attr = BlockAttributes::from_block_state(block_state);
                    if attr.has_flag(BlockFlag::FallDamageResetting) {
                        return ControlFlow::Break(true);
                    }
                }
                ControlFlow::Continue(())
            }));
            if reset_fall_distance == Some(true) {
                collision.falling_distance = 0.0;
            } else if moved.y < 0.0 {
                collision.falling_distance -= moved.y as f32;
            }
        }

        self.position += moved;
        self.velocity = moved;

        self.on_ground = hit_y && velocity.y < 0.0;
        if collision.calculate_falling && self.on_ground {
            collision.accumulated_fallen_distance += collision.falling_distance;
            collision.falling_distance = 0.0;
        }

        // Step
        if self.on_ground && (hit_x || hit_z) && collision.step_height > 0.0 {
            if collision.only_step_if_moving_forwards {
                let (yaw_sin, yaw_cos) = self.rotation.y.to_radians().sin_cos();

                let look = DVec2::new(
                    -yaw_sin,
                    yaw_cos
                );
                let dot = velocity.normalize_or_zero().xz().dot(look) as f32;
                if dot <= 0.0 {
                    return;
                }
            }

            // Move up
            let delta = DVec3::new(0.0, collision.step_height as _, 0.0);
            let aabb = collision.aabb.translate(self.position);
            let (moved_up, _, _, _) = self.generic_world().shape_cast(aabb, delta);
            self.position += moved_up;

            // Move side
            let remainder = DVec3::new(velocity.x - moved.x, 0.0, velocity.z - moved.z);
            let aabb = collision.aabb.translate(self.position);
            let (moved_side, _, _, _) = self.generic_world().shape_cast(aabb, remainder);
            self.velocity += moved_side;
            self.position += moved_side;

            // Move down
            let aabb = collision.aabb.translate(self.position);
            let (moved_down, _, _, _) = self.generic_world().shape_cast(aabb, -moved_up);
            self.position += moved_down;
        }
    }

    pub(crate) fn update_position_without_view(&mut self) {
        let old_chunk_x = self.last_chunk_x;
        let old_chunk_z = self.last_chunk_z;
        let new_chunk_x = ((self.position.x.floor() as i32) >> 4).clamp(0, self.max_chunk_x);
        let new_chunk_z = ((self.position.z.floor() as i32) >> 4).clamp(0, self.max_chunk_z);

        if old_chunk_x == new_chunk_x && old_chunk_z == new_chunk_z {
            return;
        }

        if let Some(chunk_ref) = self.chunk_ref.take() {
            let chunk = self.get_last_chunk_mut().unwrap();
            chunk.remove_entity(chunk_ref);
        }

        self.last_chunk_x = new_chunk_x;
        self.last_chunk_z = new_chunk_z;
        
        let id = self.self_id;
        if let Some(chunk) = self.get_chunk_mut(new_chunk_x, new_chunk_z) {
            self.chunk_ref = Some(chunk.insert_entity(id));
        }

        if self.remote_entities.is_empty() {
            // Nothing to do, return early
            return;
        }

        let remote_entities = unsafe { self.world.as_ref().unwrap().as_ref() }.get_remote_entities();

        let world = unsafe { self.world.as_mut().unwrap().as_mut() };
        let delta = (new_chunk_x - old_chunk_x, new_chunk_z - old_chunk_z);

        SCRATCH_BUFFERS.with(|buffers| {
            let (spawn_buffer, despawn_buffer, despawn_vec) = &mut *buffers.borrow_mut();
        
            let mut written_spawn_packets = false;
            let mut written_despawn_packets = false;

            // Write spawn/despawn for players
            chunk_view_diff::for_each_diff(delta, self.entity_view_distance, 
                |dx, dz, status| {
                    if status == ChunkDiffStatus::New {
                        let Some(chunk) = world.get_chunk_mut(old_chunk_x+dx, old_chunk_z+dz) else {
                            return;
                        };

                        if chunk.has_players() {
                            if !written_spawn_packets {
                                let mut bundle = BundledPacketBuffer::new(spawn_buffer);
                                for remote_entity in &self.remote_entities {
                                    remote_entities[remote_entity.slab_index].spawn(&mut *bundle, self.position, self.rotation.y, self.rotation.x);
                                }
                                written_spawn_packets = true;
                            }

                            chunk.write_immediately_to_players(spawn_buffer.peek_written());
                        }
                    } else {
                        let Some(chunk) = world.get_chunk_mut(old_chunk_x+dx, old_chunk_z+dz) else {
                            return;
                        };

                        if chunk.has_players() {
                            if !written_despawn_packets {
                                let mut bundle = BundledPacketBuffer::new(despawn_buffer);
                                for remote_entity in &self.remote_entities {
                                    remote_entities[remote_entity.slab_index].despawn(despawn_vec, &mut *bundle);
                                }
                                if !despawn_vec.is_empty() {
                                    bundle.write(&graphite_mc_protocol::play::clientbound::RemoveEntities {
                                        entities: Cow::Borrowed(&*despawn_vec),
                                    });
                                }

                                written_despawn_packets = true;
                            }

                            chunk.write_immediately_to_players(despawn_buffer.peek_written());
                        }
                    }
                }
            );

            spawn_buffer.clear();
            despawn_buffer.clear();
            despawn_vec.clear();
        });
    }

    pub(crate) fn update_position_with_view(&mut self, view: &mut EntityView) {
        let old_chunk_x = self.last_chunk_x;
        let old_chunk_z = self.last_chunk_z;
        let new_chunk_x = ((self.position.x.floor() as i32) >> 4).clamp(0, self.max_chunk_x);
        let new_chunk_z = ((self.position.z.floor() as i32) >> 4).clamp(0, self.max_chunk_z);

        if old_chunk_x == new_chunk_x && old_chunk_z == new_chunk_z {
            return;
        }

        if let Some(chunk_ref) = self.chunk_ref.take() {
            let chunk = self.get_last_chunk_mut().unwrap();
            chunk.remove_entity(chunk_ref);
        }

        self.last_chunk_x = new_chunk_x;
        self.last_chunk_z = new_chunk_z;

        let id = self.self_id;
        if let Some(chunk) = self.get_chunk_mut(new_chunk_x, new_chunk_z) {
            self.chunk_ref = Some(chunk.insert_entity(id));
        }

        if view.spawn.is_none() && view.despawn.is_none() && view.entity_ids.is_empty() && self.remote_entities.is_empty() {
            // Nothing to do, return early
            return;
        }
    
        let delta = (new_chunk_x - old_chunk_x, new_chunk_z - old_chunk_z);
        let remote_entities = unsafe { self.world.as_ref().unwrap().as_ref() }.get_remote_entities();
        let world = unsafe { self.world.as_mut().unwrap().as_mut() };
        let entity_ref = self.generic_world().entity_world().entity(self.self_id).unwrap();

        SCRATCH_BUFFERS.with(|buffers| {
            let (spawn_buffer, despawn_buffer, despawn_vec) = &mut *buffers.borrow_mut();
        
            let mut written_spawn_packets = false;
            let mut written_despawn_packets = false;

            // Write spawn/despawn for players
            chunk_view_diff::for_each_diff(delta, self.entity_view_distance, 
                |dx, dz, status| {
                    if status == ChunkDiffStatus::New {
                        if view.spawn.is_none() && self.remote_entities.is_empty() {
                            return;
                        }

                        let Some(chunk) = world.get_chunk_mut(old_chunk_x+dx, old_chunk_z+dz) else {
                            return;
                        };

                        if chunk.has_players() {
                            if !written_spawn_packets {
                                let mut bundle = BundledPacketBuffer::new(spawn_buffer);
                                if let Some(spawn) = view.spawn {
                                    (spawn)(entity_ref, self, &*view, &mut *bundle);
                                }
                                for remote_entity in &self.remote_entities {
                                    remote_entities[remote_entity.slab_index].spawn(&mut *bundle, self.position, self.rotation.y, self.rotation.x);
                                }
                                written_spawn_packets = true;
                            }

                            chunk.write_immediately_to_players(spawn_buffer.peek_written());
                        }
                    } else {
                        if view.despawn.is_none() && view.entity_ids.is_empty() && self.remote_entities.is_empty() {
                            return;
                        };

                        let Some(chunk) = world.get_chunk_mut(old_chunk_x+dx, old_chunk_z+dz) else {
                            return;
                        };

                        if chunk.has_players() {
                            if !written_despawn_packets {
                                let mut bundle = BundledPacketBuffer::new(despawn_buffer);
                                for remote_entity in &self.remote_entities {
                                    remote_entities[remote_entity.slab_index].despawn(despawn_vec, &mut *bundle);
                                }
                                despawn_vec.extend(&view.entity_ids);
                                if let Some(despawn) = view.despawn {
                                    (despawn)(entity_ref, self, &*view, despawn_vec, &mut *bundle);
                                }
                                if !despawn_vec.is_empty() {
                                    bundle.write(&graphite_mc_protocol::play::clientbound::RemoveEntities {
                                        entities: Cow::Borrowed(&*despawn_vec),
                                    });
                                }

                                written_despawn_packets = true;
                            }

                            chunk.write_immediately_to_players(despawn_buffer.peek_written());
                        }
                    }
                }
            );

            spawn_buffer.clear();
            despawn_buffer.clear();
            despawn_vec.clear();
        });
    }
}