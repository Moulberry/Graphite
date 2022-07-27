use bevy_ecs::{world::{EntityRef, EntityMut}, prelude::Component};
use binary::slice_serialization::SliceSerializable;
use net::{network_buffer::WriteBuffer, packet_helper};
use protocol::{play::server::{self, RemoveEntities, AddEntity}, IdentifiedPacket};
use rand::Rng;

use crate::universe::EntityId;

use super::position::{Coordinate, Rotation};

type FnPacket = fn(&mut WriteBuffer, EntityRef);

#[derive(Component, Clone)]
pub struct Viewable {
    pub coord: Coordinate,
    pub(crate) index_in_chunk_entity_slab: usize,
    pub(crate) last_chunk_x: i32,
    pub(crate) last_chunk_z: i32,
    pub(crate) buffer: *mut WriteBuffer,

    pub(crate) fn_create: FnPacket,
    //pub(crate) fn_destroy: FnPacket,

    pub(crate) destroy_buffer: WriteBuffer,
}

// Don't worry about it
unsafe impl Send for Viewable {}
unsafe impl Sync for Viewable {}

impl Viewable {
    pub fn new(coord: Coordinate, chunk_x: i32, chunk_z: i32, fn_create: FnPacket, destroy_buffer: WriteBuffer) -> Self {
        Self {
            index_in_chunk_entity_slab: 0,
            last_chunk_x: chunk_x,
            last_chunk_z: chunk_z,
            coord,
            buffer: std::ptr::null_mut(),

            fn_create,
            //fn_destroy,

            // create_buffer: WriteBuffer::with_min_capacity(64),
            destroy_buffer,
        }
    }

    // Update packets
    pub fn write_viewable_packet<'a, T>(&mut self, packet: &'a T) -> anyhow::Result<bool>
    where
        T: SliceSerializable<'a, T> + IdentifiedPacket<server::PacketId> + 'a,
    {
        if let Some(buffer) = unsafe { self.buffer.as_mut() } {
            packet_helper::write_packet(buffer, packet)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    // Create packets
    /*pub fn clear_create_packets(&mut self) {
        self.create_buffer.reset();
    }

    pub fn write_create_packet<'a, T>(&mut self, packet: &'a T) -> anyhow::Result<()>
    where
        T: SliceSerializable<'a, T> + IdentifiedPacket<server::PacketId> + 'a,
    {
        packet_helper::write_packet(&mut self.create_buffer, packet)
    }

    // Destroy packets
    pub fn clear_destroy_packets(&mut self) {
        self.destroy_buffer.reset();
    }

    pub fn write_destroy_packet<'a, T>(&mut self, packet: &'a T) -> anyhow::Result<()>
    where
        T: SliceSerializable<'a, T> + IdentifiedPacket<server::PacketId> + 'a,
    {
        packet_helper::write_packet(&mut self.destroy_buffer, packet)
    }*/
}

pub struct EntityIdHolder(EntityId);

#[derive(Component)]
pub struct BasicEntity {
    pub entity_id: EntityId,
    pub entity_type: i32,
}

#[derive(Component)]
pub struct Spinalla {
    pub direction: (f32, f32),
    pub rotation: Rotation,
}

impl EntitySpawnDefinition for BasicEntity {
    fn get_spawn_function(&mut self) -> FnPacket {
        BasicEntity::write_spawn_packet
    }

    fn get_despawn_buffer(&mut self) -> WriteBuffer {
        let mut write_buffer = WriteBuffer::with_min_capacity(8);
        let remove_packet = RemoveEntities {
            entities: vec![self.entity_id.as_i32()],
        };
        net::packet_helper::write_packet(&mut write_buffer, &remove_packet).unwrap();
        write_buffer
    }

    fn add_components(self, entity: &mut EntityMut) {
        entity.insert(self);
    }
}

impl BasicEntity {
    fn write_spawn_packet(write_buffer: &mut WriteBuffer, entity: EntityRef) {
        let viewable = entity.get::<Viewable>()
            .expect("all entities must have viewable");
        let test_entity = entity.get::<BasicEntity>()
            .expect("should have test entity!");

        let add_entity_packet = AddEntity {
            id: test_entity.entity_id.as_i32(),
            uuid: rand::thread_rng().gen(),
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
        net::packet_helper::write_packet(write_buffer, &add_entity_packet).unwrap();
    }
}

pub trait EntitySpawnDefinition {
    fn get_spawn_function(&mut self) -> FnPacket;
    fn get_despawn_buffer(&mut self) -> WriteBuffer;
    fn add_components(self, entity: &mut EntityMut);
}
