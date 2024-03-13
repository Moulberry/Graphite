use std::{borrow::Cow, cell::{RefCell, UnsafeCell}, ptr::NonNull, rc::Rc};

use downcast_rs::Downcast;
use glam::{DVec2, DVec3, Vec3Swizzles};
use graphite_binary::slice_serialization::SliceSerializable;
use graphite_mc_protocol::{play::clientbound::{self, EntityAnimation}, IdentifiedPacket};
use graphite_network::PacketBuffer;

use crate::{world::{WorldExtension, World, chunk::{Chunk, ChunkEntityRef}, EntityId, chunk_view_diff::{self, ChunkDiffStatus}}, types::AABB};

use super::entity_view_controller::EntityViewController;

thread_local! {
    static SPAWN_SCRATCH_BUFFER: RefCell<PacketBuffer> = RefCell::new(PacketBuffer::new());
    static DESPAWN_SCRATCH_BUFFER: RefCell<PacketBuffer> = RefCell::new(PacketBuffer::new());
    static DESPAWN_SCRATCH_VEC: RefCell<Vec<i32>> = RefCell::new(Vec::new());
}

pub trait GenericEntity: Downcast {
    fn tick(&mut self);
    fn write_spawn(&self, packet_buffer: &mut PacketBuffer);
    fn write_despawn(&self, despawn_list: &mut Vec<i32>, packet_buffer: &mut PacketBuffer);
    fn get_self_id(&self) -> Option<EntityId>;
    fn add_to_world(&mut self, self_id: EntityId, self_rc: Rc<UnsafeCell<dyn GenericEntity>>);
}
downcast_rs::impl_downcast!(GenericEntity);

impl <E: EntityExtension + 'static> GenericEntity for Entity<E> {
    fn tick(&mut self) {
        <Entity<E>>::tick(self)
    }

    fn write_spawn(&self, packet_buffer: &mut PacketBuffer) {
        <Entity<E>>::write_spawn(self, packet_buffer)
    }

    fn write_despawn(&self, despawn_list: &mut Vec<i32>, packet_buffer: &mut PacketBuffer) {
        <Entity<E>>::write_despawn(self, despawn_list, packet_buffer)
    }

    fn get_self_id(&self) -> Option<EntityId> {
        self.self_id.clone()
    }

    fn add_to_world(&mut self, self_id: EntityId, self_rc: Rc<UnsafeCell<dyn GenericEntity>>) {
        self.add_to_world(self_id, self_rc)
    }
}

pub trait EntityExtension: Sized + 'static {
    type World: WorldExtension;
    type View: EntityViewController<Self>;

    fn tick(entity: &mut Entity<Self>);
    fn create_view_controller(&mut self) -> Self::View;

    fn on_remove(_entity: &mut Entity<Self>) {
    }

    fn get_step_height(&self, _dot: f32) -> f32 {
        0.0
    }

    fn get_collision_width(&self) -> f32 {
        0.0
    }

    fn get_collision_height(&self) -> f32 {
        0.0
    }

    fn do_collision(&self) -> bool {
        true
    }
}

pub struct Entity<E: EntityExtension> {
    world: NonNull<World<E::World>>,
    pub(crate) self_id: Option<EntityId>,

    last_position: DVec3,
    pub position: DVec3,
    pub rotation: DVec2,

    pub velocity: DVec3,
    pub on_ground: bool,

    pub(crate) last_chunk_x: i32,
    pub(crate) last_chunk_z: i32,
    chunk_ref: Option<ChunkEntityRef>,

    pub extension: E,
    pub view: E::View
}

impl <E: EntityExtension> Entity<E> {
    pub fn new(world: &mut World<E::World>, position: DVec3, mut extension: E) -> Self {
        let view = extension.create_view_controller();

        Self {
            world: world.into(),
            self_id: None,

            last_position: position,
            position,
            rotation: DVec2::ZERO,

            velocity: DVec3::ZERO,
            on_ground: false,

            last_chunk_x: (position.x.floor() as i32) >> 4,
            last_chunk_z: (position.z.floor() as i32) >> 4,
            chunk_ref: None,
            
            extension,
            view
        }
    }

    fn add_to_world(&mut self, self_id: EntityId, self_rc: Rc<UnsafeCell<dyn GenericEntity>>) {
        if self.self_id.is_some() {
            panic!("already added to world")
        }

        self.self_id = Some(self_id);

        let exposed = E::View::get_exposed_ids(self);
        for id in exposed {
            self.world_mut().entities_by_network_id.insert(id, self_rc.clone());
        }

        let chunk_x = self.last_chunk_x;
        let chunk_z = self.last_chunk_z;
        if let Some(chunk) = self.world_mut().get_chunk_mut(chunk_x, chunk_z) {
            self.chunk_ref = Some(chunk.insert_entity(self_rc.clone()));

            self.write_viewable_immediate(Self::write_spawn);
        }
    }

    pub fn remove(&mut self) {
        if self.self_id.is_none() {
            return;
        }
        self.self_id = None;

        let chunk_x = self.last_chunk_x;
        let chunk_z = self.last_chunk_z;

        if let Some(chunk_ref) = self.chunk_ref.take() {
            let chunk = self.world_mut().get_chunk_mut(chunk_x, chunk_z).unwrap();
            chunk.remove_entity(chunk_ref);
        }

        for exposed in E::View::get_exposed_ids(self) {
            self.world_mut().entities_by_network_id.remove(&exposed);
        }

        DESPAWN_SCRATCH_BUFFER.with(|despawn_buffer| {
            let mut despawn_buffer = despawn_buffer.borrow_mut();

            DESPAWN_SCRATCH_VEC.with(|despawn_vec| {
                let mut despawn_vec = despawn_vec.borrow_mut();

                let mut written_despawn_packets = false;

                // Write despawn packets
                let view_distance = E::World::ENTITY_VIEW_DISTANCE as i32;
                for x in (chunk_x-view_distance) .. (chunk_x+view_distance+1) {
                    for z in (chunk_z-view_distance) .. (chunk_z+view_distance+1) {
                        let world = unsafe { self.world.as_mut() };
                        if let Some(chunk) = world.get_chunk_mut(x, z) {
                            if chunk.has_players() {
                                if !written_despawn_packets {
                                    self.write_despawn(&mut despawn_vec, &mut despawn_buffer);
                                    despawn_buffer.write_packet(&graphite_mc_protocol::play::clientbound::RemoveEntities {
                                        entities: Cow::Borrowed(&*despawn_vec),
                                    }).unwrap();

                                    written_despawn_packets = true;
                                }

                                chunk.write_immediately_to_players(despawn_buffer.peek_written());
                            }
                        }
                    }
                }

                despawn_buffer.clear();
                despawn_vec.clear();
            });
        });

        E::on_remove(self);
    }

    pub fn world(&self) -> &World<E::World> {
        unsafe {
            self.world.as_ref()
        }
    }

    pub fn world_mut(&mut self) -> &mut World<E::World> {
        unsafe {
            self.world.as_mut()
        }
    }

    pub fn world_ptr(&self) -> NonNull<World<E::World>> {
        self.world
    }

    pub fn write_viewable(&mut self, lambda: impl FnMut(&mut PacketBuffer)) {
        if let Some(chunk) = self.get_last_chunk_mut() {
            chunk.write_viewable(lambda);
        }
    }

    pub fn write_viewable_immediate(&mut self, mut lambda: impl FnMut(&Entity<E>, &mut PacketBuffer)) {
        let chunk_x = self.last_chunk_x;
        let chunk_z = self.last_chunk_z;

        SPAWN_SCRATCH_BUFFER.with(|spawn_buffer| {
            let mut spawn_buffer = spawn_buffer.borrow_mut();

            let mut written_packets = false;

            // Write spawn packets
            let view_distance = E::World::ENTITY_VIEW_DISTANCE as i32;
            for x in (chunk_x-view_distance) .. (chunk_x+view_distance+1) {
                for z in (chunk_z-view_distance) .. (chunk_z+view_distance+1) {
                    let world = unsafe { self.world.as_mut() };
                    if let Some(chunk) = world.get_chunk_mut(x, z) {
                        if chunk.has_players() {
                            if !written_packets {
                                lambda(self, &mut spawn_buffer);
                                written_packets = true;
                            }

                            chunk.write_immediately_to_players(spawn_buffer.peek_written());
                        }
                    }
                }
            }

            spawn_buffer.clear();
        });
    }

    pub fn add_viewable_packet<'a, I: std::fmt::Debug, T>(&mut self, packet: &'a T)
    where
        T: SliceSerializable<'a, T> + IdentifiedPacket<I> + 'a,
    {
        if let Some(chunk) = self.get_last_chunk_mut() {
            chunk.add_entity_viewable_packet(packet);
        }
    }

    pub fn play_animation(&mut self, animation: EntityAnimation) {
        if let Some(entity_id) = E::View::get_main_exposed_id(self) {
            self.add_viewable_packet(&clientbound::AnimateEntity {
                entity_id,
                animation,
            });
        }
    }

    pub fn get_last_chunk_mut(&mut self) -> Option<&mut Chunk> {
        let world = unsafe { self.world.as_mut() };
        world.get_chunk_mut(self.last_chunk_x, self.last_chunk_z)
    }

    pub fn create_collision_aabb(&self) -> Option<AABB> {
        let collision_width = self.extension.get_collision_width() as f64;
        let collision_height = self.extension.get_collision_height() as f64;

        let half_collision_width = collision_width * 0.5;
        let min = DVec3::new(self.position.x - half_collision_width, self.position.y, self.position.z - half_collision_width);
        let max = DVec3::new(self.position.x + half_collision_width, self.position.y + collision_height, self.position.z + half_collision_width);
        AABB::new(min, max)
    }

    fn tick(&mut self) {
        E::tick(self);

        let collision_width = self.extension.get_collision_width() as f64;
        let collision_height = self.extension.get_collision_height() as f64;

        if collision_width <= 0.0 || collision_height <= 0.0 || !self.extension.do_collision() {
            self.position += self.velocity;
            self.on_ground = false;
        } else {
            let aabb = self.create_collision_aabb().unwrap();
            let velocity = self.velocity;
            let (moved, hit_x, hit_y, hit_z) = self.world_mut().move_bounding_box_with_collision(aabb, velocity);
    
            self.position += moved;
            self.velocity = moved;
    
            self.on_ground = hit_y && velocity.y < 0.0;

            if self.on_ground && (hit_x || hit_z) {
                let (yaw_sin, yaw_cos) = self.rotation.y.to_radians().sin_cos();

                let look = DVec2::new(
                    yaw_sin,
                    -yaw_cos
                );

                let step_height = self.extension.get_step_height(velocity.normalize().xz().dot(look) as f32);
                if step_height > 0.0 {
                    // Move up
                    let delta = DVec3::new(0.0, step_height as _, 0.0);
                    let aabb = self.create_collision_aabb().unwrap();
                    let (moved_up, _, _, _) = self.world_mut().move_bounding_box_with_collision(aabb, delta);
                    self.position += moved_up;
    
                    // Move side
                    let remainder = DVec3::new(velocity.x - moved.x, 0.0, velocity.z - moved.z);
                    let aabb = self.create_collision_aabb().unwrap();
                    let (moved_side, _, _, _) = self.world_mut().move_bounding_box_with_collision(aabb, remainder);
                    self.velocity += moved_side;
                    self.position += moved_side;

                    // Move down
                    let aabb = self.create_collision_aabb().unwrap();
                    let (moved_down, _, _, _) = self.world_mut().move_bounding_box_with_collision(aabb, -moved_up);
                    self.position += moved_down;

                }
            }
        }

        // update position
        self.update_position();
    }

    fn write_spawn(&self, packet_buffer: &mut PacketBuffer) {
        E::View::write_spawn_packets(self, packet_buffer);
    }

    fn write_despawn(&self, despawn_list: &mut Vec<i32>, packet_buffer: &mut PacketBuffer) {
        E::View::write_despawn_packets(self, despawn_list, packet_buffer);
    }

    fn update_position(&mut self) {
        if self.get_last_chunk_mut().is_some() {
            E::View::update_position(self);
        }

        let old_chunk_x = (self.last_position.x.floor() as i32) >> 4;
        let old_chunk_z = (self.last_position.z.floor() as i32) >> 4;
        let new_chunk_x = (self.position.x.floor() as i32) >> 4;
        let new_chunk_z = (self.position.z.floor() as i32) >> 4;

        if old_chunk_x != new_chunk_x || old_chunk_z != new_chunk_z {
            if let Some(chunk_ref) = self.chunk_ref.take() {
                let chunk = self.world_mut().get_chunk_mut(old_chunk_x, old_chunk_z).unwrap();
                chunk.remove_entity(chunk_ref);
            }

            let id = self.self_id.clone().unwrap();
            self.chunk_ref = self.world_mut().put_entity_into_chunk(id, new_chunk_x, new_chunk_z);

            let delta = (new_chunk_x - old_chunk_x, new_chunk_z - old_chunk_z);

            SPAWN_SCRATCH_BUFFER.with(|spawn_buffer| {
                let mut spawn_buffer = spawn_buffer.borrow_mut();

                DESPAWN_SCRATCH_BUFFER.with(|despawn_buffer| {
                    let mut despawn_buffer = despawn_buffer.borrow_mut();

                    DESPAWN_SCRATCH_VEC.with(|despawn_vec| {
                        let mut despawn_vec = despawn_vec.borrow_mut();
    
                        let mut written_spawn_packets = false;
                        let mut written_despawn_packets = false;

                        // Write spawn/despawn for players
                        chunk_view_diff::for_each_diff(delta, E::World::ENTITY_VIEW_DISTANCE, 
                            |dx, dz, status| {
                                if status == ChunkDiffStatus::New {
                                    let world = unsafe { self.world.as_mut() };
                                    if let Some(chunk) = world.get_chunk_mut(old_chunk_x+dx, old_chunk_z+dz) {
                                        if chunk.has_players() {
                                            if !written_spawn_packets {
                                                self.write_spawn(&mut spawn_buffer);
                                                written_spawn_packets = true;
                                            }

                                            chunk.write_immediately_to_players(spawn_buffer.peek_written());
                                        }
                                    }
                                } else {
                                    let world = unsafe { self.world.as_mut() };
                                    if let Some(chunk) = world.get_chunk_mut(old_chunk_x+dx, old_chunk_z+dz) {
                                        if chunk.has_players() {
                                            if !written_despawn_packets {
                                                self.write_despawn(&mut despawn_vec, &mut despawn_buffer);
                                                despawn_buffer.write_packet(&graphite_mc_protocol::play::clientbound::RemoveEntities {
                                                    entities: Cow::Borrowed(&*despawn_vec),
                                                }).unwrap();

                                                written_despawn_packets = true;
                                            }

                                            chunk.write_immediately_to_players(despawn_buffer.peek_written());
                                        }
                                    }
                                }
                            }
                        );

                        spawn_buffer.clear();
                        despawn_buffer.clear();
                        despawn_vec.clear();
                    });
                });
            });
        }

        self.last_position = self.position;
        self.last_chunk_x = new_chunk_x;
        self.last_chunk_z = new_chunk_z;

    }

    pub fn view(&mut self) -> &mut E::View {
        &mut self.view
    }
}