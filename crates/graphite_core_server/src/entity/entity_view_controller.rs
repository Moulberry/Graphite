use graphite_binary::slice_serialization::SliceSerializable;
use graphite_mc_protocol::{play::{self, clientbound::{MoveEntityPos, MoveEntityPosRot, MoveEntityRot, TeleportEntity}}, types::ByteRotation, IdentifiedPacket};
use graphite_network::PacketBuffer;
use glam::{DVec2, DVec3};

use super::{next_entity_id, EntityExtension, Entity};

pub trait EntityViewController<E: EntityExtension> {
    fn write_spawn_packets(entity: &Entity<E>, buffer: &mut PacketBuffer);
    fn write_despawn_packets(entity: &Entity<E>, despawn_list: &mut Vec<i32>, buffer: &mut PacketBuffer);
    fn update_position(entity: &mut Entity<E>);
    fn get_exposed_ids(_entity: &mut Entity<E>) -> Vec<i32> {
        vec![]
    }
    fn get_main_exposed_id(_entity: &mut Entity<E>) -> Option<i32> {
        None
    }
}

pub struct SimpleEntityViewController {
    pub entity_id: i32,
    pub entity_type: graphite_mc_constants::entity::Entity,
    synced_position: Option<DVec3>,
    old_rotation: (u8, u8),
    teleport_time: usize,
}

impl DefaultUpdatePosition for SimpleEntityViewController {
    fn synced_position(&mut self) -> &mut Option<DVec3> {
        &mut self.synced_position
    }

    fn teleport_time(&mut self) -> &mut usize {
        &mut self.teleport_time
    }

    fn old_rotation(&mut self) -> &mut (u8, u8) {
        &mut self.old_rotation
    }
    
    fn entity_id(&self) -> i32 {
        self.entity_id
    }
}

impl SimpleEntityViewController {
    pub fn new(entity_type: graphite_mc_constants::entity::Entity) -> Self {
        Self {
            entity_id: next_entity_id(),
            entity_type,
            synced_position: None,
            old_rotation: (0, 0),
            teleport_time: 0,
        }
    }
}

impl <E: EntityExtension<View = Self>> EntityViewController<E> for SimpleEntityViewController {
    fn write_spawn_packets(entity: &Entity<E>, buffer: &mut PacketBuffer) {
        buffer.write_packet(&play::clientbound::AddEntity {
            id: entity.view.entity_id,
            uuid: rand::random(),
            entity_type: entity.view.entity_type as i32,
            x: entity.position.x,
            y: entity.position.y,
            z: entity.position.z,
            pitch: 0.0,
            yaw: 0.0,
            head_yaw: 0.0,
            data: 0,
            x_vel: 0.0,
            y_vel: 0.0,
            z_vel: 0.0,
        }).unwrap();
    }

    fn write_despawn_packets(entity: &Entity<E>, despawn_list: &mut Vec<i32>, _: &mut PacketBuffer) {
        despawn_list.push(entity.view.entity_id);
    }

    fn update_position(entity: &mut Entity<E>) {
        default_update_position(entity)
    }

    fn get_exposed_ids(entity: &mut Entity<E>) -> Vec<i32> {
        vec![entity.view.entity_id]
    }
}

pub trait DefaultUpdatePosition {
    fn synced_position(&mut self) -> &mut Option<DVec3>;
    fn teleport_time(&mut self) -> &mut usize;
    fn old_rotation(&mut self) -> &mut (u8, u8);

    fn entity_id(&self) -> i32;
    fn passenger_ids(&self) -> Vec<i32> {
        vec![]
    }
}

pub fn default_update_position<T: DefaultUpdatePosition, E: EntityExtension<View = T>>(entity: &mut Entity<E>) {
    let new_rotation = (
        ByteRotation::from_f32(entity.rotation.x as f32),
        ByteRotation::from_f32(entity.rotation.y as f32)
    );

    let Some(synced_position) = entity.view.synced_position() else {
        // Force teleport for first tick
        let teleport_packet = TeleportEntity {
            entity_id: entity.view.entity_id(),
            x: entity.position.x as _,
            y: entity.position.y as _,
            z: entity.position.z as _,
            yaw: entity.rotation.y as _,
            pitch: entity.rotation.x as _,
            on_ground: false,
        };
        let _ = entity.add_viewable_packet(&teleport_packet);

        *entity.view.old_rotation() = new_rotation;
        *entity.view.teleport_time() = 0;
        *entity.view.synced_position() = Some(entity.position);
        return;
    };
    let synced_position = *synced_position;

    let delta = entity.position - synced_position;
    let quantized = delta * 4096.0;

    *entity.view.teleport_time() = entity.view.teleport_time().wrapping_add(1);

    // Rotate passengers
    if *entity.view.old_rotation() != new_rotation {
        for entity_id in entity.view.passenger_ids() {
            let move_packet = MoveEntityRot {
                entity_id,
                yaw: entity.rotation.y as _,
                pitch: entity.rotation.x as _,
                on_ground: false,
            };
            let _ = entity.add_viewable_packet(&move_packet);
        }
    }

    if quantized.abs().max_element() < 1.0 {
        if *entity.view.old_rotation() != new_rotation {
            let move_packet = MoveEntityRot {
                entity_id: entity.view.entity_id(),
                yaw: entity.rotation.y as _,
                pitch: entity.rotation.x as _,
                on_ground: false,
            };
            let _ = entity.add_viewable_packet(&move_packet);

            *entity.view.old_rotation() = new_rotation;
        }
        return;
    }

    if quantized.min_element() <= i16::MIN as f64 || quantized.max_element() >= i16::MAX as f64 || *entity.view.teleport_time() > 20 {
        // Force teleport due to large distance or 20 ticks since last teleport
        let teleport_packet = TeleportEntity {
            entity_id: entity.view.entity_id(),
            x: entity.position.x as _,
            y: entity.position.y as _,
            z: entity.position.z as _,
            yaw: entity.rotation.y as _,
            pitch: entity.rotation.x as _,
            on_ground: false,
        };
        let _ = entity.add_viewable_packet(&teleport_packet);

        *entity.view.old_rotation() = new_rotation;
        *entity.view.teleport_time() = 0;
        *entity.view.synced_position() = Some(entity.position);
    } else {
        // Relative move
        let quantized = quantized.as_i16vec3();
        *entity.view.synced_position() = Some(synced_position + quantized.as_dvec3() * 4096.0);

        if *entity.view.old_rotation() != new_rotation {
            let move_packet = MoveEntityPosRot {
                entity_id: entity.view.entity_id(),
                delta_x: quantized.x,
                delta_y: quantized.y,
                delta_z: quantized.z,
                yaw: entity.rotation.y as _,
                pitch: entity.rotation.x as _,
                on_ground: false,
            };
            let _ = entity.add_viewable_packet(&move_packet);

            *entity.view.old_rotation() = new_rotation;
        } else {
            let move_packet = MoveEntityPos {
                entity_id: entity.view.entity_id(),
                delta_x: quantized.x,
                delta_y: quantized.y,
                delta_z: quantized.z,
                on_ground: false,
            };
            let _ = entity.add_viewable_packet(&move_packet);
        }
    }
}