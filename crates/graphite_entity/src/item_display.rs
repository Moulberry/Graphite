use std::{borrow::Cow, marker::PhantomData};

use graphite_binary::nbt::{CachedNBT, NBT, TAG_FLOAT_ID};
use graphite_core_server::{entity::{entity_view_controller::EntityViewController, next_entity_id, Entity, EntityExtension}, world::WorldExtension};
use graphite_mc_constants::{block, entity::{BlockDisplayMetadata, ItemDisplayMetadata, Metadata}, item::Item};
use graphite_mc_protocol::{play, types::ProtocolItemStack};
use graphite_network::PacketBuffer;

pub fn get_item_and_custom_model_data(entity: &NBT) -> Option<(Item, i32)> {
    if let Some(item) = entity.find_compound("item") {
        let item_string = item.find_string("id").unwrap();
        let item_u16 = graphite_mc_constants::item::string_to_u16(item_string).unwrap();

        if let Some(tag) = item.find_compound("tag") {
            if let Some(custom_model_data) = tag.find_int("CustomModelData") {
                return item_u16.try_into().ok().and_then(|item| Some((item, *custom_model_data)));
            }
        }
    }

    None
}

pub struct ItemDisplay<W: WorldExtension> {
    _phantom: PhantomData<W>,
    item: i32,
    item_nbt: CachedNBT,
    translation: (f32, f32, f32),
    left_rotation: (f32, f32, f32, f32),
    scale: (f32, f32, f32),
    right_rotation: (f32, f32, f32, f32),
}

impl <W: WorldExtension> ItemDisplay<W> {
    pub fn new(entity: NBT) -> Self {
        let translation;
        let left_rotation;
        let scale;
        let right_rotation;

        if let Some(transformation) = entity.find_compound("transformation") {
            translation = if let Some(translation) = transformation.find_list("translation", TAG_FLOAT_ID) {
                (
                    *translation.get_float(0).unwrap(),
                    *translation.get_float(1).unwrap(),
                    *translation.get_float(2).unwrap(),
                )
            } else {
                (0.0, 0.0, 0.0)
            };
            left_rotation = if let Some(left_rotation) = transformation.find_list("left_rotation", TAG_FLOAT_ID) {
                (
                    *left_rotation.get_float(0).unwrap(),
                    *left_rotation.get_float(1).unwrap(),
                    *left_rotation.get_float(2).unwrap(),
                    *left_rotation.get_float(3).unwrap(),
                )
            } else {
                (0.0, 0.0, 0.0, 1.0)
            };
            scale = if let Some(scale) = transformation.find_list("scale", TAG_FLOAT_ID) {
                (
                    *scale.get_float(0).unwrap(),
                    *scale.get_float(1).unwrap(),
                    *scale.get_float(2).unwrap(),
                )
            } else {
                (1.0, 1.0, 1.0)
            };
            right_rotation = if let Some(right_rotation) = transformation.find_list("right_rotation", TAG_FLOAT_ID) {
                (
                    *right_rotation.get_float(0).unwrap(),
                    *right_rotation.get_float(1).unwrap(),
                    *right_rotation.get_float(2).unwrap(),
                    *right_rotation.get_float(3).unwrap(),
                )
            } else {
                (0.0, 0.0, 0.0, 1.0)
            };
        } else {
            translation = (0.0, 0.0, 0.0);
            left_rotation = (0.0, 0.0, 0.0, 1.0);
            scale = (1.0, 1.0, 1.0);
            right_rotation = (0.0, 0.0, 0.0, 1.0);
        }

        if let Some(item) = entity.find_compound("item") {
            let item_string = item.find_string("id").unwrap();
            let item_u16 = graphite_mc_constants::item::string_to_u16(item_string).unwrap();

            let item_nbt = if let Some(tag) = item.find_compound("tag") {
                tag.clone_nbt().into()
            } else {
                CachedNBT::new()
            };

            Self {
                _phantom: PhantomData,
                item: item_u16 as i32,
                item_nbt,
                translation,
                left_rotation,
                scale,
                right_rotation
            }
        } else {
            Self {
                _phantom: PhantomData,
                item: 0,
                item_nbt: CachedNBT::new(),
                translation,
                left_rotation,
                scale,
                right_rotation
            }
        }
    }
}

impl <W: WorldExtension> EntityExtension for ItemDisplay<W> {
    type World = W;
    type View = ItemDisplayView;

    fn tick(_: &mut Entity<Self>) {
    }

    fn create_view_controller(&mut self) -> Self::View {
        ItemDisplayView {
            entity_id: next_entity_id()
        }
    }
}

pub struct ItemDisplayView {
    entity_id: i32
}

impl <W: WorldExtension> EntityViewController<ItemDisplay<W>> for ItemDisplayView {
    fn write_spawn_packets(entity: &Entity<ItemDisplay<W>>, buffer: &mut PacketBuffer) {
        buffer.write_packet(&play::clientbound::AddEntity {
            id: entity.view.entity_id,
            uuid: rand::random(),
            entity_type: graphite_mc_constants::entity::Entity::ItemDisplay as i32,
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

        let mut metadata = ItemDisplayMetadata::default();
        metadata.set_item_stack(ProtocolItemStack {
            item: entity.extension.item,
            count: 1,
            nbt: Cow::Borrowed(&entity.extension.item_nbt),
        });

        metadata.set_translation(entity.extension.translation);
        metadata.set_left_rotation(entity.extension.left_rotation);
        metadata.set_scale(entity.extension.scale);
        metadata.set_right_rotation(entity.extension.right_rotation);

        metadata.write_metadata_changes_packet(entity.view.entity_id, buffer).unwrap();
    }

    fn write_despawn_packets(entity: &Entity<ItemDisplay<W>>, despawn_list: &mut Vec<i32>, _: &mut PacketBuffer) {
        despawn_list.push(entity.view.entity_id)
    }

    fn update_position(_: &mut Entity<ItemDisplay<W>>) {
        // nothing
    }
}