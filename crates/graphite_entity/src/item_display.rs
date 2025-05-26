use glam::DVec3;
use graphite_binary::nbt::{CompoundRef, NBT, TAG_DOUBLE_ID};
use graphite_core_server::entity::{entity_view::{self, EntityView}, EntityBase};
use graphite_mc_constants::{entity::ItemDisplayMetadata, item::Item};
use graphite_mc_protocol::{play::clientbound::{AddEntity, SetEntityData}, types::{data_component::{CustomModelData, DataComponentMap}, ItemStack}, IdentifiedPacket};
use graphite_network::PacketBuffer;
use hecs::{EntityBuilder, EntityRef};

use crate::transform::Transform;

pub fn get_item_and_custom_model_data(entity: CompoundRef<'_>) -> Option<(Item, i32)> {
    if let Some(item) = entity.find_compound("item") {
        let item_string = item.find_string("id").unwrap();
        let item_u16 = graphite_mc_constants::item::string_to_u16(item_string).unwrap();
        let item_type: Item = item_u16.try_into().ok()?;

        if let Some(tag) = item.find_compound("tag") {
            let custom_model_data: Option<i32> = tag.find_numeric("CustomModelData");
            if let Some(custom_model_data) = custom_model_data {
                return Some((item_type, custom_model_data));
            }
        }
        if let Some(tag) = item.find_compound("components") {
            let custom_model_data: Option<i32> = tag.find_numeric("minecraft:custom_model_data");
            if let Some(custom_model_data) = custom_model_data {
                return Some((item_type, custom_model_data));
            }
        }
    }

    None
}

pub fn get_custom_model(entity: CompoundRef<'_>) -> Option<String> {
    let item = entity.find_compound("item")?;
    let components = item.find_compound("components")?;
    let item_model = components.find_string("minecraft:item_model")?;

    Some(item_model.clone())
}

#[derive(Clone)]
pub struct ItemDisplayEntityView {
    metadata: ItemDisplayMetadata,
    last_transform: Transform,
    pub transform: Transform,

    synced_position: Option<DVec3>,
    old_rotation: (u8, u8),
    teleport_time: usize,
}

unsafe impl Send for ItemDisplayEntityView {}
unsafe impl Sync for ItemDisplayEntityView {}

impl ItemDisplayEntityView {
    pub fn new_static(item_stack: ItemStack, transform: Transform) -> EntityBuilder {
        let mut metadata = ItemDisplayMetadata::default();
        metadata.set_item_stack(Some(item_stack.as_bytes()));

        let mut builder = EntityBuilder::new();
        Self::add(&mut builder, metadata, transform, false);
        builder
    }

    pub fn load_item_from_display(entity: &NBT) -> Option<ItemStack> {
        let Some(entity) = entity.as_compound() else {
            return None;
        };

        if let Some(item) = entity.find_compound("item") {
            let item_stack = ItemStack::load_from_nbt(item)?;
            Some(item_stack)
        } else {
            None
        }
    }

    pub fn load_static(entity: CompoundRef<'_>) -> Option<(DVec3, EntityBuilder)> {
        let transform = Transform::load_from_entity(entity);

        let pos = entity.find_list("Pos", TAG_DOUBLE_ID)?;

        let x = *pos.get_double(0).unwrap();
        let y = *pos.get_double(1).unwrap();
        let z = *pos.get_double(2).unwrap();

        if let Some(item) = entity.find_compound("item") {
            let item_stack = ItemStack::load_from_nbt(item)?;

            let mut metadata = ItemDisplayMetadata::default();
            metadata.set_item_stack(Some(item_stack.as_bytes()));

            let mut builder = EntityBuilder::new();
            Self::add(&mut builder, metadata, transform, false);
            Some((DVec3::new(x, y, z), builder))
        } else {
            None
        }
    }

    pub fn add(builder: &mut EntityBuilder, metadata: ItemDisplayMetadata, transform: Transform, update_position: bool) {
        if builder.has::<Self>() || builder.has::<EntityView>() {
            panic!("duplicate view");
        }
        builder.add(Self {
            metadata,
            last_transform: transform.clone(),
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
        let item_display = &mut *entity.get::<&mut ItemDisplayEntityView>().unwrap();
        AddEntity {
            id: view.entity_ids[0],
            uuid: rand::random(),
            entity_type: graphite_mc_constants::entity::Entity::ItemDisplay as i32,
            x: base.position.x,
            y: base.position.y,
            z: base.position.z,
            pitch: base.rotation.x as f32,
            yaw: base.rotation.y as f32,
            head_yaw: base.rotation.y as f32,
            ..Default::default()
        }.write_packet(buffer);

        item_display.metadata.set_pos_rot_interpolation_duration(2);
        item_display.metadata.set_transformation_interpolation_duration(2);

        item_display.metadata.set_translation(item_display.transform.translation);
        item_display.metadata.set_left_rotation(item_display.transform.left_rotation);
        item_display.metadata.set_scale(item_display.transform.scale);
        item_display.metadata.set_right_rotation(item_display.transform.right_rotation);

        let scale_xz = item_display.transform.scale.0.max(item_display.transform.scale.2);
        item_display.metadata.set_width(4.0 * scale_xz);
        item_display.metadata.set_height(4.0 * item_display.transform.scale.1);

        SetEntityData::write_non_default(&mut item_display.metadata, view.entity_ids[0], buffer);
    }

    pub fn update(entity: EntityRef, base: &mut EntityBase, view: &EntityView) {
        let item_display = &mut *entity.get::<&mut ItemDisplayEntityView>().unwrap();
        entity_view::default_position_update(base, view.entity_ids[0], &[],
            &mut item_display.synced_position, &mut item_display.old_rotation, &mut item_display.teleport_time, false);
            
        if item_display.last_transform != item_display.transform {
            item_display.last_transform = item_display.transform.clone();

            item_display.metadata.set_translation(item_display.transform.translation);
            item_display.metadata.set_left_rotation(item_display.transform.left_rotation);
            item_display.metadata.set_scale(item_display.transform.scale);
            item_display.metadata.set_right_rotation(item_display.transform.right_rotation);
            item_display.metadata.set_transformation_interpolation_start_delta_ticks(0);

            base.write_viewable(|buffer| {
                SetEntityData::write_changes(&mut item_display.metadata, view.entity_ids[0], buffer);
            })
        }
    }
}