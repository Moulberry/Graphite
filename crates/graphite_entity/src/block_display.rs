

use glam::DVec3;
use graphite_binary::nbt::CompoundRef;
use graphite_core_server::entity::{entity_view::{self, EntityView}, EntityBase};
use graphite_mc_constants::entity::BlockDisplayMetadata;
use graphite_mc_protocol::{play::clientbound::{AddEntity, SetEntityData}, IdentifiedPacket};
use graphite_network::PacketBuffer;
use hecs::{EntityBuilder, EntityRef};

use crate::transform::Transform;

pub fn get_block_state_id(entity: CompoundRef<'_>) -> i32 {
    if let Some(block_state) = entity.find_compound("block_state") {
        graphite_mc_constants::block::parse_block_state(block_state) as i32
    } else {
        0
    }
}

#[derive(Clone)]
pub struct BlockDisplayEntityView {
    pub block: i32,
    transform: Transform,

    synced_position: Option<DVec3>,
    old_rotation: (u8, u8),
    teleport_time: usize,
}

impl BlockDisplayEntityView {
    pub fn new_static(block: i32, transform: Transform) -> EntityBuilder {
        let mut builder = EntityBuilder::new();
        Self::add(&mut builder, block, transform, false);
        builder
    }

    pub fn load_static(entity: CompoundRef<'_>) -> Option<EntityBuilder> {
        let transform = Transform::load_from_entity(entity);

        if let Some(block_state) = entity.find_compound("block_state") {
            let block_state = graphite_mc_constants::block::parse_block_state(block_state) as i32;

            let mut builder = EntityBuilder::new();
            Self::add(&mut builder, block_state, transform, false);
            Some(builder)
        } else {
            None
        }
    }

    pub fn add(builder: &mut EntityBuilder, block: i32, transform: Transform, update_position: bool) {
        if builder.has::<Self>() || builder.has::<EntityView>() {
            panic!("duplicate view");
        }
        builder.add(Self {
            block,
            transform,

            synced_position: None,
            old_rotation: (0, 0),
            teleport_time: 0
        });
        builder.add(EntityView::new(
            1,
            Some(Self::spawn),
            None,
            if update_position {
                Some(Self::update)
            } else {
                None
            },
        ));
    }

    pub fn spawn(entity: EntityRef, base: &EntityBase, view: &EntityView, buffer: &mut PacketBuffer) {
        let block_display = entity.get::<&BlockDisplayEntityView>().unwrap();
        AddEntity {
            id: view.entity_ids[0],
            uuid: rand::random(),
            entity_type: graphite_mc_constants::entity::Entity::BlockDisplay as i32,
            x: base.position.x,
            y: base.position.y,
            z: base.position.z,
            pitch: base.rotation.x as f32,
            yaw: base.rotation.y as f32,
            head_yaw: base.rotation.y as f32,
            ..Default::default()
        }.write_packet(buffer);

        let mut metadata = BlockDisplayMetadata::default();
        metadata.set_block_state(block_display.block);

        metadata.set_pos_rot_interpolation_duration(2);
        metadata.set_transformation_interpolation_duration(2);

        metadata.set_translation(block_display.transform.translation);
        metadata.set_left_rotation(block_display.transform.left_rotation);
        metadata.set_scale(block_display.transform.scale);
        metadata.set_right_rotation(block_display.transform.right_rotation);

        metadata.set_width(4.0);
        metadata.set_height(4.0);

        SetEntityData::write_changes(&mut metadata, view.entity_ids[0], buffer);
    }

    pub fn update(entity: EntityRef, base: &mut EntityBase, view: &EntityView) {
        let block_display = &mut *entity.get::<&mut BlockDisplayEntityView>().unwrap();
        entity_view::default_position_update(base, view.entity_ids[0], &[],
            &mut block_display.synced_position, &mut block_display.old_rotation, &mut block_display.teleport_time, false)
    }
}