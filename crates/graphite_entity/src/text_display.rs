

use glam::DVec3;

use graphite_binary::nbt::{CompoundRef, EncodedNBT};
use graphite_core_server::{entity::{entity_view::{self, EntityView}, EntityBase}};
use graphite_mc_constants::{entity::TextDisplayMetadata, types::BillboardConstraint};
use graphite_mc_protocol::{play::{self, clientbound::SetEntityData}, types::text::TextComponent, IdentifiedPacket};
use graphite_network::PacketBuffer;
use hecs::{EntityBuilder, EntityRef};

use crate::transform::Transform;

pub struct TextDisplayEntityView {
    text: EncodedNBT,
    background_color: i32,
    shadow: bool,
    billboard: BillboardConstraint,

    transform: Transform,

    synced_position: Option<DVec3>,
    old_rotation: (u8, u8),
    teleport_time: usize,
}

impl TextDisplayEntityView {
    pub fn new_static(text: TextComponent<'static>, background_color: i32, shadow: bool, billboard: BillboardConstraint, transform: Transform) -> EntityBuilder {
        let mut builder = EntityBuilder::new();
        Self::add(&mut builder, text, background_color, shadow, billboard, transform, false);
        builder
    }

    pub fn add(builder: &mut EntityBuilder, text: TextComponent<'static>, background_color: i32, shadow: bool, billboard: BillboardConstraint, transform: Transform, update_position: bool) {
        let text_display = Self {
            text: text.to_encoded_nbt(),
            background_color,
            shadow,
            billboard,

            transform,

            synced_position: None,
            old_rotation: (0, 0),
            teleport_time: 0
        };

        builder.add(text_display);

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

    fn spawn(entity: EntityRef, base: &EntityBase, view: &EntityView, buffer: &mut PacketBuffer) {
        let text_display = entity.get::<&TextDisplayEntityView>().unwrap();

        play::clientbound::AddEntity {
            id: view.entity_ids[0],
            uuid: rand::random(),
            entity_type: graphite_mc_constants::entity::Entity::TextDisplay as i32,
            x: base.position.x,
            y: base.position.y,
            z: base.position.z,
            ..Default::default()
        }.write_packet(buffer);

        let mut metadata = TextDisplayMetadata::default();

        metadata.set_text(text_display.text.clone());
        metadata.set_background_color(text_display.background_color);
        metadata.set_billboard_render_constraints(text_display.billboard as u8);
        metadata.set_pos_rot_interpolation_duration(2);
        metadata.set_transformation_interpolation_duration(2);
        if text_display.shadow {
            metadata.set_style_flags(1);
        }

        metadata.set_translation(text_display.transform.translation);
        metadata.set_left_rotation(text_display.transform.left_rotation);
        metadata.set_scale(text_display.transform.scale);
        metadata.set_right_rotation(text_display.transform.right_rotation);

        metadata.set_width(4.0);
        metadata.set_height(4.0);

        SetEntityData::write_changes(&mut metadata, view.entity_ids[0], buffer);
    }

    fn update(entity: EntityRef, base: &mut EntityBase, view: &EntityView) {
        let text_display = &mut *entity.get::<&mut TextDisplayEntityView>().unwrap();
        entity_view::default_position_update(base, view.entity_ids[0], &[],
            &mut text_display.synced_position, &mut text_display.old_rotation, &mut text_display.teleport_time, false)
    }
}