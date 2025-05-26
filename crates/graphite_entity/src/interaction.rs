use std::borrow::Cow;

use graphite_core_server::entity::{entity_view::EntityView, EntityBase};
use graphite_mc_constants::entity::InteractionMetadata;
use graphite_mc_protocol::{play::clientbound::{AddEntity, SetEntityData, SetPassengers}, IdentifiedPacket};
use graphite_network::PacketBuffer;
use hecs::EntityRef;

#[derive(Clone)]
pub struct InteractionEntityView {
    pub width: f32,
    pub height: f32,
    pub parent_index: Option<usize>,
    pub interaction_index: usize
}

impl InteractionEntityView {
    pub fn spawn(entity: EntityRef, base: &EntityBase, view: &EntityView, buffer: &mut PacketBuffer) {
        let interaction = entity.get::<&InteractionEntityView>().unwrap();
        AddEntity {
            id: view.entity_ids[interaction.interaction_index],
            uuid: rand::random(),
            entity_type: graphite_mc_constants::entity::Entity::Interaction as i32,
            x: base.position.x,
            y: base.position.y,
            z: base.position.z,
            pitch: base.rotation.x as f32,
            yaw: base.rotation.y as f32,
            head_yaw: base.rotation.y as f32,
            ..Default::default()
        }.write_packet(buffer);

        let mut metadata = InteractionMetadata::default();
        metadata.set_width(interaction.width);
        metadata.set_height(interaction.height);
        SetEntityData::write_changes(&mut metadata, view.entity_ids[interaction.interaction_index], buffer);

        if let Some(parent_index) = interaction.parent_index {
            SetPassengers {
                entity_id: view.entity_ids[parent_index],
                passengers: Cow::Borrowed(&[view.entity_ids[interaction.interaction_index]]),
            }.write_packet(buffer);
        }
    }
}