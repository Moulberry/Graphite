use bevy_ecs::{
    prelude::Component,
    world::{EntityMut, EntityRef},
};
use binary::slice_serialization::SliceSerializable;
use net::{network_buffer::WriteBuffer, packet_helper};
use protocol::{
    play::server::{self, AddEntity, AddPlayer, PlayerInfo, PlayerInfoAddPlayer, RemoveEntities},
    types::{GameProfile, GameProfileProperty},
    IdentifiedPacket,
};
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
    pub(crate) destroy_buffer: WriteBuffer,
}

// Don't worry about it
unsafe impl Send for Viewable {}
unsafe impl Sync for Viewable {}

impl Viewable {
    pub fn new(
        coord: Coordinate,
        chunk_x: i32,
        chunk_z: i32,
        fn_create: FnPacket,
        destroy_buffer: WriteBuffer,
    ) -> Self {
        Self {
            index_in_chunk_entity_slab: 0,
            last_chunk_x: chunk_x,
            last_chunk_z: chunk_z,
            coord,
            buffer: std::ptr::null_mut(),

            fn_create,
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
}

pub trait EntitySpawnDefinition {
    fn get_spawn_function(&mut self) -> FnPacket;
    fn get_despawn_buffer(&mut self) -> WriteBuffer;
    fn add_components(self, entity: &mut EntityMut);
}

pub struct EntityIdHolder(EntityId);

/*#[derive(Component)]
pub struct EntityMetadata {
    pub metadata: Box<dyn Metadata>
}

unsafe impl Sync for EntityMetadata {}
unsafe impl Send for EntityMetadata {}

impl EntityMetadata {
    pub fn downcast_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.metadata.as_any_mut().downcast_mut()
    }
}*/

#[derive(Component)]
pub struct PlayerNPC {
    pub entity_id: EntityId,
    pub uuid: u128,
    pub username: String,
}

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
        let viewable = entity
            .get::<Viewable>()
            .expect("all entities must have viewable");
        let basic_entity = entity
            .get::<BasicEntity>()
            .expect("should have test entity!");

        let add_entity_packet = AddEntity {
            id: basic_entity.entity_id.as_i32(),
            uuid: rand::thread_rng().gen(), // todo: don't randomize here
            entity_type: basic_entity.entity_type,
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

impl EntitySpawnDefinition for PlayerNPC {
    fn get_spawn_function(&mut self) -> FnPacket {
        PlayerNPC::write_spawn_packet
    }

    fn get_despawn_buffer(&mut self) -> WriteBuffer {
        let mut write_buffer = WriteBuffer::with_min_capacity(8);

        // Remove Entity Packet
        let remove_entity_packet = RemoveEntities {
            entities: vec![self.entity_id.as_i32()],
        };
        net::packet_helper::write_packet(&mut write_buffer, &remove_entity_packet).unwrap();

        // Remove Player Info
        let remove_info_packet = PlayerInfo::RemovePlayer {
            uuids: vec![self.uuid],
        };
        net::packet_helper::write_packet(&mut write_buffer, &remove_info_packet).unwrap();

        write_buffer
    }

    fn add_components(self, entity: &mut EntityMut) {
        entity.insert(self);
    }
}

impl PlayerNPC {
    fn write_spawn_packet(write_buffer: &mut WriteBuffer, entity: EntityRef) {
        let viewable = entity
            .get::<Viewable>()
            .expect("all entities must have viewable");
        let player_npc = entity.get::<PlayerNPC>().expect("should have test entity!");

        let profile = GameProfile {
            uuid: player_npc.uuid,
            username: player_npc.username.clone(),
            properties: vec![
                GameProfileProperty {
                    id: "textures".into(),
                    value: "ewogICJ0aW1lc3RhbXAiIDogMTY1OTAyMDI4NjQ0OCwKICAicHJvZmlsZUlkIiA6ICJkMGUwNWRlNzYwNjc0NTRkYmVhZWM2ZDE5ZDg4NjE5MSIsCiAgInByb2ZpbGVOYW1lIiA6ICJNb3VsYmVycnkiLAogICJzaWduYXR1cmVSZXF1aXJlZCIgOiB0cnVlLAogICJ0ZXh0dXJlcyIgOiB7CiAgICAiU0tJTiIgOiB7CiAgICAgICJ1cmwiIDogImh0dHA6Ly90ZXh0dXJlcy5taW5lY3JhZnQubmV0L3RleHR1cmUvYmNlMTU1MjI0ZWE0YmM0OWE4ZTkxOTA3MzdjYjA0MTdkOGE3YzM4YTAzN2Q4ZDAzODJkZGU0ODI5YzEwMzU5MCIsCiAgICAgICJtZXRhZGF0YSIgOiB7CiAgICAgICAgIm1vZGVsIiA6ICJzbGltIgogICAgICB9CiAgICB9CiAgfQp9".into(),
                    signature: Some("XMgxJ45DZlaKr3BozEJ9tYUpqqhN/WIvHt8T8KGnbYjUFGq5q3WOodpR/2hlBE5dgTL+wk3QFXXuBYzDmcKVPl3Nh/Qv3ZqETOZQ1hC5hLTpNwCKH55QGRqEQYwLEZ+4fz2bdTqd+nISehl6fEwHLb3mSXIj7n/ICxJ0jPw9+1BDndY2omKRjnD8G3VRf3gAhcwMw5mCTy3RMOa+3VIe4YTUqQFSOqQ7H1JTmD1mzXbGaqJaOg6DOFlI+nXXNuajfqr2TEiK78ieZk78mzvYB5K/5NH2NttKmuDYNVyR9u5f9IyRpEFba0tIEC1DpfbSu7TgNb5tIXTxzr9W0sG+OyVN2+/hO1vxejvYpSFJki/O1E5UHLKAilVr4IVjnpMNsY/6TS6C83UTz3UGXghSuSiX77xMGikzgJmUaNFjUoCe1jzdu3aBA/PPCXVQh17CBilVWFFUE5qapKphp9rPD2KpaOjPyRv9dWEx1c0VFhAUWDcoM4/6dnqdpR8AGzZSBLNpAL+DfaZ83qwfZ8GIqvDdYbvz09A9DHEOhgy3qoPvgwCMKTdsTsrQhVOVxKo0s0hNDiDu3ZKpF3SA2OXcaRES+B/xWSQ9Lcq1G9++v+0TWiKS+3oyecUCIQcdrQZQDxKXgVPUUo1XXUEgCjEdCUy0OuWmSQCrSBhWG6bfguk=".into())
                }
            ]
        };

        let packet = PlayerInfo::AddPlayer {
            values: vec![PlayerInfoAddPlayer {
                profile,
                gamemode: 1,
                ping: 69,
                display_name: Some("{\"text\": \"Ya Boi\"}"),
                signature_data: None,
            }],
        };
        net::packet_helper::write_packet(write_buffer, &packet).unwrap();

        let add_player_packet = AddPlayer {
            id: player_npc.entity_id.as_i32(),
            uuid: player_npc.uuid,
            x: viewable.coord.x as _,
            y: viewable.coord.y as _,
            z: viewable.coord.z as _,
            yaw: 0.0,
            pitch: 0.0,
        };
        net::packet_helper::write_packet(write_buffer, &add_player_packet).unwrap();
    }
}
