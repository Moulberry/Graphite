use std::{borrow::Cow, collections::HashMap};

use glam::{DVec3, Mat4, Quat, Vec3};
use graphite_binary::nbt::EncodedNBT;
use graphite_core_server::{entity::{entity_view::{self, EntityView}, remote_entity::RemoteEntity, EntityBase}, types::AABB, world::RemoteEntityId};
use graphite_mc_constants::{builtin::Attribute, entity::{Entity, EntityAndMetadata, InteractionMetadata, ItemDisplayMetadata, Metadata, SlimeMetadata, TextDisplayMetadata}, item::Item};
use graphite_mc_protocol::{play::{self, clientbound::{AttributeEntry, RemoveEntities, SetEntityData, SetPassengers, UpdateAttributes}}, types::{data_component::{CustomModelData, DataComponentMap}, ItemStack}, IdentifiedPacket};
use graphite_network::PacketBuffer;
use hecs::{EntityBuilder, EntityRef};
use serde::Deserialize;

use crate::transform::Transform;

pub struct CustomEntityView {
    pub bones: Vec<Bone>,
    
    playing_animation: Option<PlayingAnimation>,
    layered_animations: Vec<PlayingAnimation>,

    last_variant: usize,
    current_variant: usize,
    variants: &'static Vec<String>,
    animations: &'static HashMap<String, (usize, usize, bool)>,
    bone_name_to_index: &'static HashMap<String, usize>,

    base_transform: Mat4,

    dirty_bones: u64,

    hitbox: Hitbox,
    hitbox_entity: Option<RemoteEntityId>,

    nametag_changed: bool,
    had_nametag: bool,
    nametag: Option<EncodedNBT>,

    synced_position: Option<DVec3>,
    old_rotation: (u8, u8),
    teleport_time: usize,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Hitbox {
    Slime(f64),
    Interaction(f32, f32),
    None
}

impl Hitbox {
    pub const fn entity_type(self) -> Option<Entity> {
        match self {
            Hitbox::Slime(_) => Some(Entity::Slime),
            Hitbox::Interaction(_, _) => Some(Entity::Interaction),
            Hitbox::None => None,
        }
    }

    pub const fn height(self) -> f64 {
        match self {
            Hitbox::Slime(size) => size,
            Hitbox::Interaction(_, height) => height as f64,
            Hitbox::None => 0.0,
        }
    }
}

struct PlayingAnimation {
    index: usize,
    length: usize,
    frame: usize,
    hold_on_last_frame: bool,
    loop_animation: bool,
    name: &'static str
}

impl CustomEntityView {
    pub fn add(builder: &mut EntityBuilder, definition: &'static CustomEntityDefinition, variant: Option<&'static str>, hitbox: Hitbox) {
        Self::add_with_transform(builder, definition, variant, hitbox, Transform::default())
    }

    pub fn add_with_transform(builder: &mut EntityBuilder, definition: &'static CustomEntityDefinition, variant: Option<&'static str>, hitbox: Hitbox, transform: Transform) {
        let mut bones: Vec<Bone> = Vec::new();

        for bone in &definition.bones {
            let mut rotation = rotation_to_quaternion(Vec3::from(bone.rotation));
            let mut offset = glam::Vec3::from(bone.offset);
            let mut position = glam::Vec3::from(bone.offset);

            if let Some(parent_index) = bone.parent {
                let parent = bones.get(parent_index).unwrap();
                rotation = parent.transforms.rotation.mul_quat(rotation);
                offset = offset - glam::Vec3::from(definition.bones.get(parent_index).unwrap().offset);

                let rotated_offset = parent.transforms.rotation.mul_vec3(offset);
                position = parent.transforms.position + rotated_offset;
            }

            bones.push(Bone {
                custom_model_data: bone.custom_model_data,
                parent: bone.parent,
                
                additional_rotation: Quat::IDENTITY,
                last_dirtied_additional_rotation: Quat::IDENTITY,
                transforms: BoneTransforms {
                    rotation,
                    position,
                    scale: Vec3::ONE,
                },
                
                base_rotation_from_parent: Vec3::from(bone.rotation),
                base_offset_from_parent: offset,
                
                bounds: bone.bounds,
                animations: &bone.animations
            });
        }

        // Ensure the 64-bit-wide dirty_bones field is sufficient
        assert!(bones.len() <= 64);

        let view = EntityView::new_with_primary_index(
            definition.bones.len() + 2,
            Some(Self::spawn),
            None,
            Some(Self::update),
            1
        );

        let mut custom = Self {
            synced_position: None,
            old_rotation: (0, 0),
            teleport_time: 0,
            bones,
            playing_animation: None,
            last_variant: 0,
            current_variant: 0,

            base_transform: transform.to_matrix(),

            dirty_bones: 0,

            hitbox,
            hitbox_entity: None,

            nametag_changed: false,
            had_nametag: false,
            nametag: None,

            variants: &definition.variants,
            animations: &definition.animations,
            layered_animations: Vec::new(),
            bone_name_to_index: &definition.bone_name_to_index
        };
        if let Some(variant) = variant {
            custom.set_variant(variant);
        }

        builder.add(custom);
        builder.add(view);
    }

    pub fn set_variant(&mut self, name: &str) {
        for (index, variant_name) in self.variants.iter().enumerate() {
            if variant_name == name {
                self.current_variant = index;
                return;
            }
        }
        self.current_variant = 0;
    }

    pub fn get_variant_id(&self, name: &str) -> usize {
        for (index, variant_name) in self.variants.iter().enumerate() {
            if variant_name == name {
                return index;
            }
        }
        return 0;
    }

    pub fn set_nametag(&mut self, text: Option<EncodedNBT>) {
        self.nametag = text;
        self.nametag_changed = true;
    }

    pub fn clear_layered_animations(&mut self) {
        self.layered_animations.clear();
    }

    pub fn play_layered_animation_if_finished(&mut self, animation: &'static str) {
        if let Some((index, length, _)) = self.animations.get(animation) {
            for layered in &self.layered_animations {
                if layered.index == *index {
                    return;
                }
            }

            self.layered_animations.push(PlayingAnimation {
                index: *index,
                length: *length,
                frame: 0,
                hold_on_last_frame: false,
                loop_animation: false,
                name: animation
            });
        } else {
            panic!("unknown animation: {}", animation);
        }
    }

    pub fn play_animation_force(&mut self, animation: &'static str) {
        if let Some((index, length, hold_on_last_frame)) = self.animations.get(animation) {
            self.playing_animation = Some(PlayingAnimation {
                index: *index,
                length: *length,
                frame: 0,
                hold_on_last_frame: *hold_on_last_frame,
                loop_animation: false,
                name: animation
            });
        } else {
            panic!("unknown animation: {}", animation);
        }
    }

    pub fn make_animation_hold_on_last_frame(&mut self) {
        if let Some(playing_animation) = self.playing_animation.as_mut() {
            playing_animation.hold_on_last_frame = true;
        }
    }

    pub fn make_animation_loop(&mut self) {
        if let Some(playing_animation) = self.playing_animation.as_mut() {
            playing_animation.loop_animation = true;
        }
    }

    pub fn get_playing_animation_name(&self) -> Option<&'static str> {
        self.playing_animation.as_ref().map(|animation| animation.name)
    }

    pub fn play_animation_if_finished(&mut self, animation: &'static str) {
        if let Some(playing_animation) = &self.playing_animation {
            if playing_animation.frame > playing_animation.length {
                self.play_animation_force(animation);
            }
        } else {
            self.play_animation_force(animation);
        }
    }

    pub fn get_hitbox(&self) -> Hitbox {
        self.hitbox
    }

    pub fn remove_hitbox(&mut self) {
        self.hitbox = Hitbox::None;
    }

    pub fn get_hitbox_entity(&self) -> Option<RemoteEntityId> {
        self.hitbox_entity
    }

    pub fn set_additional_rotation(&mut self, bone_name: &str, rotation: Quat) {
        if !rotation.is_finite() {
            return;
        }

        if let Some(&index) = self.bone_name_to_index.get(bone_name) {
            let bone = &mut self.bones[index];
            if bone.additional_rotation != rotation {
                bone.additional_rotation = rotation;

                // Prevent excessive packet usage by only dirtying bones if the angle change is visually significant
                if bone.last_dirtied_additional_rotation.angle_between(rotation) > 1.0_f32.to_radians() {
                    bone.last_dirtied_additional_rotation = rotation;
                    self.dirty_bones |= 1 << index;
                }
            }
        }
    }

    fn calculate_transform_matrix(&self, bone: &Bone) -> Transform {
        let mut matrix = self.base_transform.clone();

        matrix = matrix * Mat4::from_translation(bone.transforms.position);
        matrix = matrix * Mat4::from_quat(bone.transforms.rotation);
        matrix = matrix * Mat4::from_scale(bone.transforms.scale);

        matrix = matrix * Mat4::from_scale(Vec3::new(16.0, 16.0, 16.0));

        Transform::from_matrix(matrix)
    }

    fn spawn(entity: EntityRef, base: &EntityBase, view: &EntityView, buffer: &mut PacketBuffer) {
        let custom = entity.get::<&CustomEntityView>().unwrap();

        for (index, bone) in custom.bones.iter().enumerate() {
            let entity_id = view.entity_ids[index];

            play::clientbound::AddEntity {
                id: entity_id,
                uuid: rand::random(),
                entity_type: graphite_mc_constants::entity::Entity::ItemDisplay as i32,
                x: base.position.x,
                y: base.position.y,
                z: base.position.z,
                pitch: base.rotation.x as _,
                yaw: 180.0 + base.rotation.y as f32,
                ..Default::default()
            }.write_packet(buffer);

            let item_stack = ItemStack::new_with_custom_model_data(Item::Stick,
                bone.custom_model_data as i32 + custom.current_variant as i32);

            let mut metadata = ItemDisplayMetadata::default();
            metadata.set_item_stack(Some(item_stack.as_bytes()));

            let transform = custom.calculate_transform_matrix(bone);

            metadata.set_translation(transform.translation);
            metadata.set_left_rotation(transform.left_rotation);
            metadata.set_scale(transform.scale);
            metadata.set_right_rotation(transform.right_rotation);
            metadata.set_pos_rot_interpolation_duration(2);
            metadata.set_transformation_interpolation_duration(2);

            match custom.hitbox {
                Hitbox::Slime(size) => {
                    metadata.set_width(size as f32 + 1.0);
                    metadata.set_height(size as f32 + 1.0);
                },
                Hitbox::Interaction(width, height) => {
                    metadata.set_width(width as f32 + 1.0);
                    metadata.set_height(height as f32 + 1.0);
                },
                Hitbox::None => {
                    metadata.set_width(4.0);
                    metadata.set_height(4.0);
                },
            }

            SetEntityData::write_changes(&mut metadata, entity_id, buffer);
        }

        custom.spawn_extra(base.position, view, buffer, true);
    }

    fn spawn_extra(&self, pos: DVec3, view: &EntityView, buffer: &mut PacketBuffer, initial: bool) {
        let nametag_id = view.entity_ids[view.entity_ids.len() - 1];

        let mut update_passengers = false;

        if let Some(nametag) = &self.nametag {
            if !self.had_nametag || initial {
                update_passengers = true;

                play::clientbound::AddEntity {
                    id: nametag_id,
                    uuid: rand::random(),
                    entity_type: Entity::TextDisplay as i32,
                    x: pos.x,
                    y: pos.y,
                    z: pos.z,
                    pitch: 0.0,
                    yaw: 0.0,
                    ..Default::default()
                }.write_packet(buffer);
            }

            let mut metadata = TextDisplayMetadata::default();
            metadata.set_text(nametag.clone());
            metadata.set_pos_rot_interpolation_duration(2);
            metadata.set_billboard_render_constraints(3);
            metadata.set_background_color(0);
            SetEntityData::write_changes(&mut metadata, nametag_id, buffer);

        } else if self.had_nametag && !initial {
            update_passengers = true;

            RemoveEntities {
                entities: Cow::Borrowed(&[nametag_id])
            }.write_packet(buffer);
        }

        if update_passengers || initial {
            let max = if self.nametag.is_some() {
                view.entity_ids.len() - 1
            } else {
                view.entity_ids.len() - 2
            };

            SetPassengers {
                entity_id: view.entity_ids[0],
                passengers: Cow::Borrowed(&view.entity_ids[1..=max]),
            }.write_packet(buffer);
        }
    }

    fn update(entity: EntityRef, base: &mut EntityBase, view: &EntityView) {
        let custom = &mut *entity.get::<&mut CustomEntityView>().unwrap();

        let size = custom.bones.len();

        let variant_changed;
        if custom.current_variant != custom.last_variant {
            custom.last_variant = custom.current_variant;
            variant_changed = true;
        } else {
            variant_changed = false;
        }

        let mut animation_changed = false;
        if let Some(playing_animation) = &mut custom.playing_animation {
            if playing_animation.frame > playing_animation.length {
                if playing_animation.loop_animation {
                    animation_changed = true;
                    playing_animation.frame = 0;
                } else {
                    animation_changed = !playing_animation.hold_on_last_frame;
                    custom.playing_animation = None;
                }
            } else {
                animation_changed = true;
                playing_animation.frame += 1;
            }
        }

        custom.layered_animations.retain_mut(|layered| {
            animation_changed = true;
            if layered.frame > layered.length {
                return false;
            } else {
                layered.frame += 1;
                return true;
            }
        });

        if animation_changed || variant_changed || custom.dirty_bones != 0 {
            for index in 0..size {
                let mut metadata = ItemDisplayMetadata::default();

                if animation_changed {
                    if let Some(playing_animation) = &custom.playing_animation {
                        let bone = &custom.bones[index];

                        let animation = &bone.animations[playing_animation.index];
                        let animation_frame = playing_animation.frame - 1;

                        let mut rotation_from_parent = bone.base_rotation_from_parent;
                        let mut offset_from_parent = bone.base_offset_from_parent;
                        let mut scale_from_parent = Vec3::ONE;

                        let mut should_update_transform = false;

                        if let Some(offsets) = apply_animation(animation, animation_frame) {
                            if let Some(rotation) = offsets.rotation {
                                rotation_from_parent += rotation;
                                should_update_transform = true;
                            }
                            if let Some(position) = offsets.position {
                                offset_from_parent += position;
                                should_update_transform = true;
                            }
                            if let Some(scale) = offsets.scale {
                                scale_from_parent *= scale;
                                should_update_transform = true;
                            }
                        }

                        for layered in &custom.layered_animations {
                            let animation = &bone.animations[layered.index];
                            let animation_frame = layered.frame - 1;

                            if let Some(offsets) = apply_animation(animation, animation_frame) {
                                if let Some(rotation) = offsets.rotation {
                                    rotation_from_parent += rotation;
                                    should_update_transform = true;
                                }
                                if let Some(position) = offsets.position {
                                    offset_from_parent += position;
                                    should_update_transform = true;
                                }
                                if let Some(scale) = offsets.scale {
                                    scale_from_parent *= scale;
                                    should_update_transform = true;
                                }
                            }
                        }

                        if !should_update_transform {
                            if custom.dirty_bones & (1 << index) != 0 {
                                should_update_transform = true;
                            } else if let Some(parent_index) = bone.parent {
                                if custom.dirty_bones & (1 << parent_index) != 0 {
                                    should_update_transform = true;
                                }
                            }
                        }

                        if should_update_transform {
                            let transforms = custom.calculate_final_transforms(bone,
                                rotation_from_parent, offset_from_parent, scale_from_parent, animation.global_rotation);

                            let bone = &mut custom.bones[index];

                            if bone.transforms == transforms {
                                custom.dirty_bones &= !(1 << index);
                            } else {
                                custom.dirty_bones |= 1 << index;
                                bone.transforms = transforms;
                                bone.last_dirtied_additional_rotation = bone.additional_rotation;

                                let transform = custom.calculate_transform_matrix(&custom.bones[index]);

                                metadata.set_translation(transform.translation);
                                metadata.set_left_rotation(transform.left_rotation);
                                metadata.set_scale(transform.scale);
                                metadata.set_right_rotation(transform.right_rotation);
                                metadata.set_transformation_interpolation_duration(2);
                                metadata.set_transformation_interpolation_start_delta_ticks(0);
                            }
                        }
                    } else {
                        let bone = &custom.bones[index];

                        let transforms = custom.calculate_final_transforms(bone,
                            bone.base_rotation_from_parent, bone.base_offset_from_parent, Vec3::ONE, false);

                        let bone = &mut custom.bones[index];

                        if bone.transforms != transforms {
                            bone.transforms = transforms;
                            bone.last_dirtied_additional_rotation = bone.additional_rotation;

                            let transform = custom.calculate_transform_matrix(&custom.bones[index]);

                            metadata.set_translation(transform.translation);
                            metadata.set_left_rotation(transform.left_rotation);
                            metadata.set_scale(transform.scale);
                            metadata.set_right_rotation(transform.right_rotation);
                            metadata.set_transformation_interpolation_duration(5);
                            metadata.set_transformation_interpolation_start_delta_ticks(0);
                        }
                    }
                } else if custom.dirty_bones != 0 {
                    let mut should_update_transform = false;

                    let bone = &custom.bones[index];

                    if custom.dirty_bones & (1 << index) != 0 {
                        should_update_transform = true;
                    } else if let Some(parent_index) = bone.parent {
                        if custom.dirty_bones & (1 << parent_index) != 0 {
                            should_update_transform = true;
                        }
                    }

                    if should_update_transform {
                        let bone = &custom.bones[index];

                        let transforms = custom.calculate_final_transforms(bone,
                            bone.base_rotation_from_parent, bone.base_offset_from_parent, Vec3::ONE, false);

                        let bone = &mut custom.bones[index];

                        if bone.transforms == transforms {
                            custom.dirty_bones &= !(1 << index);
                        } else {
                            custom.dirty_bones |= 1 << index;
                            bone.transforms = transforms;
                            bone.last_dirtied_additional_rotation = bone.additional_rotation;

                            let transform = custom.calculate_transform_matrix(&custom.bones[index]);

                            metadata.set_translation(transform.translation);
                            metadata.set_left_rotation(transform.left_rotation);
                            metadata.set_scale(transform.scale);
                            metadata.set_right_rotation(transform.right_rotation);
                            metadata.set_transformation_interpolation_duration(2);
                            metadata.set_transformation_interpolation_start_delta_ticks(0);
                        }
                    }
                }

                if variant_changed {
                    let bone = &custom.bones[index];

                    let item_stack = ItemStack::new_with_custom_model_data(Item::Stick,
                        bone.custom_model_data as i32 + custom.current_variant as i32);

                    metadata.set_item_stack(Some(item_stack.as_bytes()));
                }

                if metadata.has_changed() {
                    let entity_id = view.entity_ids[index];

                    base.write_viewable(|buffer| {
                        SetEntityData::write_changes(&mut metadata, entity_id, buffer);
                    });
                }
            }
        }

        custom.dirty_bones = 0;

        if custom.nametag_changed {
            let pos = base.position;
            base.write_viewable(|buffer| {
                custom.spawn_extra(pos, view, buffer, false);
            });

            custom.had_nametag = custom.nametag.is_some();
        }
        if custom.hitbox_entity.is_none() && custom.hitbox != Hitbox::None {
            match custom.hitbox {
                Hitbox::Slime(slime_size) => {
                    let mut metadata = SlimeMetadata::default();

                    let slime_hitbox = slime_size/0.52;
                    let slime_hitbox_int = slime_hitbox.round() as i32;
                    let slime_hitbox_mult = slime_hitbox / slime_hitbox_int as f64;

                    metadata.set_id_size(slime_hitbox_int);

                    let attributes = vec![AttributeEntry {
                        id: Attribute::Scale,
                        value: slime_hitbox_mult,
                        modifiers: vec![]
                    }];

                    let remote_entity = RemoteEntity::new_with_attributes(EntityAndMetadata::Slime(metadata), attributes);
                    custom.hitbox_entity = Some(base.spawn_remote_entity(remote_entity));                    
                },
                Hitbox::Interaction(width, height) => {
                    let mut metadata = InteractionMetadata::default();
                    metadata.set_width(width);
                    metadata.set_height(height);
                    metadata.set_response(true);

                    let remote_entity = RemoteEntity::new(EntityAndMetadata::Interaction(metadata));
                    custom.hitbox_entity = Some(base.spawn_remote_entity(remote_entity));
                },
                Hitbox::None => {},
            };
        } else if custom.hitbox_entity.is_some() && custom.hitbox == Hitbox::None {
            base.remove_remote_entity(custom.hitbox_entity.take().unwrap());
        }

        let max = if custom.nametag.is_some() {
            view.entity_ids.len() - 1
        } else {
            view.entity_ids.len() - 2
        };

        entity_view::default_position_update(base, view.entity_ids[0],
            &view.entity_ids[1..=max], &mut custom.synced_position, &mut custom.old_rotation,
            &mut custom.teleport_time, true);
    }

    fn calculate_final_transforms(&self, bone: &Bone, rotation_from_parent: Vec3, offset_from_parent: Vec3, scale_from_parent: Vec3, global_rotation: bool) -> BoneTransforms {
        let new_rotation;
        let new_position;
        let new_scale;

        if let Some(parent_index) = bone.parent {
            let parent = self.bones.get(parent_index).unwrap();
            if global_rotation {
                new_rotation = rotation_to_quaternion(rotation_from_parent);
            } else {
                new_rotation = parent.transforms.rotation.mul_quat(rotation_to_quaternion(rotation_from_parent));
            }
            let rotated_offset = parent.transforms.rotation.mul_vec3(offset_from_parent);
            new_position = parent.transforms.position + rotated_offset * parent.transforms.scale;
            new_scale = scale_from_parent * parent.transforms.scale;
        } else {
            new_rotation = rotation_to_quaternion(rotation_from_parent);
            new_position = offset_from_parent;
            new_scale = scale_from_parent;
        }

        let new_rotation = new_rotation.mul_quat(bone.additional_rotation);
        BoneTransforms {
            rotation: new_rotation,
            position: new_position,
            scale: new_scale
        }
    }
}

#[derive(PartialEq)]
pub struct BoneTransforms {
    pub rotation: Quat,
    pub position: Vec3,
    pub scale: Vec3
}

struct AnimationOffsets {
    rotation: Option<Vec3>,
    position: Option<Vec3>,
    scale: Option<Vec3>
}

fn rotation_to_quaternion(rotation: Vec3) -> Quat {
    let rot_x = rotation.x.to_radians();
    let rot_y = rotation.y.to_radians();
    let rot_z = rotation.z.to_radians();
    Quat::from_euler(glam::EulerRot::ZYX, rot_z, -rot_y, -rot_x)
}

fn apply_animation(animation: &BoneAnimation, animation_frame: usize) -> Option<AnimationOffsets> {
    if animation_frame >= animation.frames.len() {
        return None;
    }

    let mut rotation_offset = None;
    let mut position_offset = None;
    let mut scale_multiplier = None;

    let frame = &animation.frames[animation_frame];

    if let Some(rotation) = frame.rotation {
        rotation_offset = Some(Vec3::from(rotation));
    } else {
        let mut before = None;
        let mut before_delta = 0;
        let mut after = None;
        let mut after_delta = 0;
        
        // Find before
        for i in (0..animation_frame).rev() {
            if let Some(frame) = animation.frames.get(i) {
                if frame.rotation.is_none() {
                    continue;
                }
                before = Some(frame);
                before_delta = animation_frame - i;
                break;
            }
        }

        // Find after
        for i in animation_frame..animation.frames.len() {
            if let Some(frame) = animation.frames.get(i) {
                if frame.rotation.is_none() {
                    continue;
                }
                after = Some(frame);
                after_delta = i - animation_frame;
                break;
            }
        }

        if let Some(before) = before {
            if let Some(after) = after {
                let before_rotation = before.rotation.unwrap();
                let after_rotation = after.rotation.unwrap();
    
                let before_rotation = glam::Vec3::from(before_rotation);
                let after_rotation = glam::Vec3::from(after_rotation);
    
                let before_delta = before_delta as f32;
                let after_delta = after_delta as f32;
                let lerp_amount = before_delta / (before_delta + after_delta);
                let rotation = before_rotation.lerp(after_rotation, lerp_amount);
    
                rotation_offset = Some(rotation);
            } else {
                rotation_offset = Some(Vec3::from(before.rotation.unwrap()));
            }
        } else if let Some(after) = after {
            rotation_offset = Some(Vec3::from(after.rotation.unwrap()));
        }
    }

    if let Some(position) = frame.position {
        position_offset = Some(Vec3::from(position));
    } else {
        let mut before = None;
        let mut before_delta = 0;
        let mut after = None;
        let mut after_delta = 0;

        // Find before
        for i in (0..animation_frame).rev() {
            if let Some(frame) = animation.frames.get(i) {
                if frame.position.is_none() {
                    continue;
                }
                before = Some(frame);
                before_delta = animation_frame - i;
                break;
            }
        }

        // Find after
        for i in animation_frame..animation.frames.len() {
            if let Some(frame) = animation.frames.get(i) {
                if frame.position.is_none() {
                    continue;
                }
                after = Some(frame);
                after_delta = i - animation_frame;
                break;
            }
        }

        if let Some(before) = before {
            if let Some(after) = after {
                let before_position = before.position.unwrap();
                let after_position = after.position.unwrap();

                let before_position = glam::Vec3::from(before_position);
                let after_position = glam::Vec3::from(after_position);

                let before_delta = before_delta as f32;
                let after_delta = after_delta as f32;
                let lerp_amount = before_delta / (before_delta + after_delta);
                let position = before_position.lerp(after_position, lerp_amount);

                position_offset = Some(position);
            } else {
                position_offset = Some(Vec3::from(before.position.unwrap()));
            }
        } else if let Some(after) = after {
            position_offset = Some(Vec3::from(after.position.unwrap()));
        }
    }

    if let Some(scale) = frame.scale {
        scale_multiplier = Some(Vec3::new(scale.0, scale.1, scale.2));
    } else {
        let mut before = None;
        let mut before_delta = 0;
        let mut after = None;
        let mut after_delta = 0;

        // Find before
        for i in (0..animation_frame).rev() {
            if let Some(frame) = animation.frames.get(i) {
                if frame.scale.is_none() {
                    continue;
                }
                before = Some(frame);
                before_delta = animation_frame - i;
                break;
            }
        }

        // Find after
        for i in animation_frame..animation.frames.len() {
            if let Some(frame) = animation.frames.get(i) {
                if frame.scale.is_none() {
                    continue;
                }
                after = Some(frame);
                after_delta = i - animation_frame;
                break;
            }
        }

        if let Some(before) = before {
            if let Some(after) = after {
                let before_scale = before.scale.unwrap();
                let after_scale = after.scale.unwrap();
    
                let before_scale = glam::Vec3::from(before_scale);
                let after_scale = glam::Vec3::from(after_scale);
    
                let before_delta = before_delta as f32;
                let after_delta = after_delta as f32;
                let lerp_amount = before_delta / (before_delta + after_delta);
                let scale = before_scale.lerp(after_scale, lerp_amount);
    
                scale_multiplier = Some(scale);
            } else {
                scale_multiplier = Some(Vec3::from(before.scale.unwrap()));
            }
        } else if let Some(after) = after {
            scale_multiplier = Some(Vec3::from(after.scale.unwrap()));
        }
    }

    Some(AnimationOffsets {
        rotation: rotation_offset,
        position: position_offset,
        scale: scale_multiplier,
    })
}

pub struct Bone {
    pub custom_model_data: isize,
    pub parent: Option<usize>,

    pub additional_rotation: glam::Quat,
    pub last_dirtied_additional_rotation: glam::Quat,
    pub transforms: BoneTransforms,

    pub base_rotation_from_parent: glam::Vec3,
    pub base_offset_from_parent: glam::Vec3,
    
    pub bounds: (f32, f32, f32, f32, f32, f32),
    animations: &'static Vec<BoneAnimation>
}

#[derive(Deserialize)]
pub struct CustomEntityDefinition {
    pub bones: Vec<CustomEntityBone>,
    variants: Vec<String>,
    animations: HashMap<String, (usize, usize, bool)>,
    bone_name_to_index: HashMap<String, usize>
}

impl CustomEntityDefinition {
    pub fn from_string(str: &str) -> Option<Self> {
        serde_json::from_str(str).ok()
    }
}

#[derive(Deserialize)]
pub struct CustomEntityBone {
    pub custom_model_data: isize,
    pub parent: Option<usize>,
    pub bounds: (f32, f32, f32, f32, f32, f32),
    pub rotation: (f32, f32, f32),
    pub offset: (f32, f32, f32),
    animations: Vec<BoneAnimation>
}

#[derive(Deserialize, Clone)]
struct BoneAnimation {
    global_rotation: bool,
    frames: Vec<BoneAnimationFrame>
}

#[derive(Deserialize, Clone)]
struct BoneAnimationFrame {
    rotation: Option<(f32, f32, f32)>,
    position: Option<(f32, f32, f32)>,
    scale: Option<(f32, f32, f32)>,
}
