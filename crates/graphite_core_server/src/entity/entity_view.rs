use glam::DVec3;
use graphite_mc_protocol::{play::clientbound::{AddEntity, EntityPositionSync, MoveEntityPos, MoveEntityPosRot, MoveEntityRot, TeleportEntity}, types::ByteRotation, IdentifiedPacket};
use graphite_network::PacketBuffer;
use hecs::{EntityBuilder, EntityRef};

use crate::entity::next_entity_ids;

use super::EntityBase;

type EntityViewSpawn = fn(entity: EntityRef, base: &EntityBase, view: &EntityView, buffer: &mut PacketBuffer);
type EntityViewDespawn = fn(entity: EntityRef, base: &EntityBase, view: &EntityView, despawn_vec: &mut Vec<i32>, buffer: &mut PacketBuffer);
type EntityViewUpdate = fn(entity: EntityRef, base: &mut EntityBase, view: &EntityView);

pub struct EntityView {
    pub entity_ids: Vec<i32>,
    pub primary_entity_id: Option<i32>,
    pub spawn: Option<EntityViewSpawn>,
    pub despawn: Option<EntityViewDespawn>,
    pub update: EntityViewUpdate,
}

impl EntityView {
    pub fn new(entity_id_count: usize, spawn: Option<EntityViewSpawn>, despawn: Option<EntityViewDespawn>, update: Option<EntityViewUpdate>) -> Self {
        Self::new_with_primary_index(entity_id_count, spawn, despawn, update, 0)
    }

    pub fn new_with_primary_index(entity_id_count: usize, spawn: Option<EntityViewSpawn>, despawn: Option<EntityViewDespawn>,
            update: Option<EntityViewUpdate>, primary_index: usize) -> Self {
        let entity_ids = next_entity_ids(entity_id_count);
        let primary_entity_id = if entity_ids.len() > primary_index {
            Some(entity_ids[primary_index])
        } else {
            None
        };

        Self {
            entity_ids,
            primary_entity_id,
            spawn,
            despawn,
            update: if let Some(update) = update {
                update
            } else {
                Self::update_noop
            },
        }
    }

    pub fn update_noop(_entity: EntityRef, _base: &mut EntityBase, _view: &EntityView) {}
}

pub struct SimpleEntityView {
    entity_type: graphite_mc_constants::entity::Entity,
    synced_position: Option<DVec3>,
    old_rotation: (u8, u8),
    teleport_time: usize,
}

impl SimpleEntityView {
    pub fn add(builder: &mut EntityBuilder, entity_type: graphite_mc_constants::entity::Entity) {
        if builder.has::<Self>() || builder.has::<EntityView>() {
            panic!("duplicate view");
        }
        builder.add(Self {
            entity_type,
            synced_position: None,
            old_rotation: (0, 0),
            teleport_time: 0
        });
        builder.add(EntityView::new(1, Some(Self::spawn), None, Some(Self::update)));
    }

    pub fn spawn(entity: EntityRef, base: &EntityBase, view: &EntityView, buffer: &mut PacketBuffer) {
        let simple_entity_view = entity.get::<&SimpleEntityView>().unwrap();
        AddEntity {
            id: view.entity_ids[0],
            uuid: rand::random(),
            entity_type: simple_entity_view.entity_type as i32,
            x: base.position.x,
            y: base.position.y,
            z: base.position.z,
            pitch: base.rotation.x as f32,
            yaw: base.rotation.y as f32,
            head_yaw: base.rotation.y as f32,
            ..Default::default()
        }.write_packet(buffer);
    }

    pub fn update(entity: EntityRef, base: &mut EntityBase, view: &EntityView) {
        let simple = &mut *entity.get::<&mut SimpleEntityView>().unwrap();
        default_position_update(base, view.entity_ids[0], &[],
            &mut simple.synced_position, &mut simple.old_rotation, &mut simple.teleport_time, false)
    }
}

pub fn default_position_update(entity_base: &mut EntityBase, root: i32, passengers: &[i32],
        synced_position: &mut Option<DVec3>, old_rotation: &mut (u8, u8), teleport_time: &mut usize, flip_yaw: bool) {
    let yaw = if flip_yaw {
        180.0 + entity_base.rotation.y as f32
    } else {
        entity_base.rotation.y as f32
    };

    let new_rotation = (
        ByteRotation::from_f32(entity_base.rotation.x as f32),
        ByteRotation::from_f32(yaw)
    );

    let Some(synced_position) = synced_position else {
        // Force teleport for first tick
        let teleport_packet = EntityPositionSync {
            entity_id: root,
            x: entity_base.position.x as _,
            y: entity_base.position.y as _,
            z: entity_base.position.z as _,
            x_vel: entity_base.velocity.x,
            y_vel: entity_base.velocity.y,
            z_vel: entity_base.velocity.z,
            yaw,
            pitch: entity_base.rotation.x as _,
            on_ground: false,
        };
        entity_base.add_viewable_packet(&teleport_packet);

        *old_rotation = new_rotation;
        *teleport_time = 0;
        *synced_position = Some(entity_base.position);
        return;
    };

    let delta = entity_base.position - *synced_position;
    let quantized = delta * 4096.0;

    // Rotate passengers
    if *old_rotation != new_rotation {
        for entity_id in passengers {
            let move_packet = MoveEntityRot {
                entity_id: *entity_id,
                yaw,
                pitch: entity_base.rotation.x as _,
                on_ground: false,
            };
            entity_base.add_viewable_packet(&move_packet);
        }
    }

    if *teleport_time < 400 {
        *teleport_time += 1;
    }

    if quantized.abs().max_element() < 1.0 {
        if *old_rotation != new_rotation {
            let move_packet = MoveEntityRot {
                entity_id: root,
                yaw,
                pitch: entity_base.rotation.x as _,
                on_ground: false,
            };
            entity_base.add_viewable_packet(&move_packet);

            *old_rotation = new_rotation;
        }
        return;
    }

    if quantized.min_element() <= i16::MIN as f64 || quantized.max_element() >= i16::MAX as f64 || *teleport_time >= 400 {
        // Force teleport due to large distance or 20 seconds since last teleport
        let teleport_packet = EntityPositionSync {
            entity_id: root,
            x: entity_base.position.x as _,
            y: entity_base.position.y as _,
            z: entity_base.position.z as _,
            x_vel: entity_base.velocity.x,
            y_vel: entity_base.velocity.y,
            z_vel: entity_base.velocity.z,
            yaw,
            pitch: entity_base.rotation.x as _,
            on_ground: false,
        };
        entity_base.add_viewable_packet(&teleport_packet);

        *old_rotation = new_rotation;
        *teleport_time = 0;
        *synced_position = entity_base.position;
    } else {
        // Relative move
        let quantized = quantized.as_i16vec3();
        *synced_position += quantized.as_dvec3() / 4096.0;

        if *old_rotation != new_rotation {
            let move_packet = MoveEntityPosRot {
                entity_id: root,
                delta_x: quantized.x,
                delta_y: quantized.y,
                delta_z: quantized.z,
                yaw,
                pitch: entity_base.rotation.x as _,
                on_ground: false,
            };
            entity_base.add_viewable_packet(&move_packet);

            *old_rotation = new_rotation;
        } else {
            let move_packet = MoveEntityPos {
                entity_id: root,
                delta_x: quantized.x,
                delta_y: quantized.y,
                delta_z: quantized.z,
                on_ground: false,
            };
            entity_base.add_viewable_packet(&move_packet);
        }
    }
}