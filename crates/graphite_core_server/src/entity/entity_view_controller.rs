use graphite_mc_protocol::play::{clientbound::{TeleportEntity, MoveEntityPosRot}, self};
use graphite_network::PacketBuffer;
use glam::DVec3;

use super::{next_entity_id, EntityExtension, Entity};

pub trait EntityViewController<E: EntityExtension> {
    fn write_spawn_packets(entity: &Entity<E>, buffer: &mut PacketBuffer);
    fn write_despawn_packets(entity: &Entity<E>, despawn_list: &mut Vec<i32>, buffer: &mut PacketBuffer);
    fn update_position(entity: &mut Entity<E>);
}

pub struct DebugEntityViewController {
    pub entity_id: i32
}

impl DebugEntityViewController {
    pub fn new() -> Self {
        Self {
            entity_id: next_entity_id()
        }
    }
}

impl <E: EntityExtension<View = Self>> EntityViewController<E> for DebugEntityViewController {
    fn write_spawn_packets(entity: &Entity<E>, buffer: &mut PacketBuffer) {
        buffer.write_packet(&play::clientbound::AddEntity {
            id: entity.view.entity_id,
            uuid: 8172638172638,
            entity_type: graphite_mc_constants::entity::Entity::Zombie as i32,
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
        let delta = entity.position - entity.synced_position;
        let quantized = delta * 4096.0;

        if quantized.min_element() <= i16::MIN as f64 || quantized.max_element() >= i16::MAX as f64 {
            // Force teleport
            let teleport_packet = TeleportEntity {
                entity_id: 1,
                x: entity.position.x as _,
                y: entity.position.y as _,
                z: entity.position.z as _,
                yaw: 0.0,
                pitch: 0.0,
                on_ground: false,
            };
            let _ = entity.add_viewable_packet(&teleport_packet);

            entity.synced_position = entity.position;
        } else if quantized.abs().max_element() >= 1.0 {
            // Relative move
            let quantized = quantized.as_i16vec3();
            entity.synced_position += quantized.as_dvec3() * 4096.0;

            let move_packet = MoveEntityPosRot {
                entity_id: 1,
                delta_x: quantized.x,
                delta_y: quantized.y,
                delta_z: quantized.z,
                yaw: 0.0,
                pitch: 0.0,
                on_ground: false,
            };
            let _ = entity.add_viewable_packet(&move_packet);
        }
    }
}