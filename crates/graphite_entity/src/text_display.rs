use std::{borrow::Cow, marker::PhantomData};

use graphite_binary::nbt::{CachedNBT, NBT, TAG_FLOAT_ID};
use graphite_core_server::{entity::{entity_view_controller::EntityViewController, next_entity_id, Entity, EntityExtension}, text::TextComponent, world::WorldExtension};
use graphite_mc_constants::{block, entity::{BlockDisplayMetadata, ItemDisplayMetadata, Metadata, TextDisplayMetadata}, item::Item};
use graphite_mc_protocol::{play, types::ProtocolItemStack};
use graphite_network::PacketBuffer;

pub struct TextDisplayView {
    entity_id: i32,
    text: String
}

impl TextDisplayView {
    pub fn new(text: String) -> Self {
        Self {
            entity_id: next_entity_id(),
            text
        }
    }
}

impl <E: EntityExtension> EntityViewController<E> for TextDisplayView
where
    E: EntityExtension<View = Self>
{
    fn write_spawn_packets(entity: &Entity<E>, buffer: &mut PacketBuffer) {
        buffer.write_packet(&play::clientbound::AddEntity {
            id: entity.view.entity_id,
            uuid: rand::random(),
            entity_type: graphite_mc_constants::entity::Entity::TextDisplay as i32,
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

        let mut metadata = TextDisplayMetadata::default();
        let component = TextComponent {
            text: &entity.view.text,
            font: None,
            color: None,
        };

        metadata.set_text(component.to_nbt().into());
        metadata.set_billboard_render_constraints(3);

        metadata.write_metadata_changes_packet(entity.view.entity_id, buffer).unwrap();
    }

    fn write_despawn_packets(entity: &Entity<E>, despawn_list: &mut Vec<i32>, _: &mut PacketBuffer) {
        despawn_list.push(entity.view.entity_id)
    }

    fn update_position(_: &mut Entity<E>) {
        // nothing
    }
}