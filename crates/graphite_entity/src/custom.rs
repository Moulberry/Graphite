use std::{borrow::Cow, collections::{BTreeMap, HashMap}};

use glam::{DVec3, Vec3};
use graphite_binary::{nbt::CachedNBT, slice_serialization::{SliceSerializable, VarInt}};
use graphite_core_server::entity::{entity_view_controller::EntityViewController, next_entity_id, Entity, EntityExtension};
use graphite_mc_constants::entity::{InteractionMetadata, ItemDisplayMetadata, Metadata, SlimeMetadata};
use graphite_mc_protocol::{play::{self, clientbound::{MoveEntityPos, MoveEntityPosRot, MoveEntityRot, SetEntityData, SetPassengers, TeleportEntity}}, types::{ByteRotation, ProtocolItemStack}, IdentifiedPacket};
use graphite_network::PacketBuffer;
use serde::Deserialize;

pub struct CustomEntityViewController {
    root_bone: i32,
    pub interaction_id: i32,
    bone_ids: Vec<i32>,
    synced_position: Option<DVec3>,
    old_rotation: (u8, u8),
    teleport_time: usize,
    bones: Vec<Bone>,
    playing_animation: Option<PlayingAnimation>,
    last_variant: usize,
    current_variant: usize,

    slime_size: i32,
    variants: Vec<String>,
    animations: HashMap<String, (usize, usize)>
}

struct PlayingAnimation {
    index: usize,
    length: usize,
    frame: usize
}

impl CustomEntityViewController {
    pub fn new(definition: CustomEntityDefinition, slime_size: i32) -> Self {
        let mut bones: Vec<Bone> = Vec::new();

        for bone in &definition.bones {
            let mut rotation = glam::Quat::from_euler(glam::EulerRot::ZYX,
                bone.rotation.2.to_radians(), -bone.rotation.1.to_radians(), -bone.rotation.0.to_radians());
            let rotation_from_parent = rotation.clone();
            let mut offset = glam::Vec3::from(bone.offset);
            let mut position = glam::Vec3::from(bone.offset);

            if let Some(parent_index) = bone.parent {
                let parent = bones.get(parent_index).unwrap();
                rotation = parent.rotation.mul_quat(rotation);
                offset = offset - glam::Vec3::from(definition.bones.get(parent_index).unwrap().offset);

                let rotated_offset = parent.rotation.mul_vec3(offset);
                position = parent.position + rotated_offset;
            }

            bones.push(Bone {
                custom_model_data: bone.custom_model_data,
                parent: bone.parent,
                rotation,
                position,
                raw_rotation: bone.rotation,
                rotation_from_parent,
                offset_from_parent: offset,
                animations: bone.animations.clone()
            });
        }

        Self {
            root_bone: next_entity_id(),
            interaction_id: next_entity_id(),
            bone_ids: vec![(); definition.bones.len() - 1].iter().map(|_| next_entity_id()).collect(),
            synced_position: None,
            old_rotation: (0, 0),
            teleport_time: 0,
            bones,
            playing_animation: None,
            last_variant: 0,
            current_variant: 0,

            slime_size,
            variants: definition.variants,
            animations: definition.animations,
        }
    }

    pub fn set_variant(&mut self, name: &str) {
        for (index, variant_name) in self.variants.iter().enumerate() {
            if variant_name == name {
                self.current_variant = index;
                break;
            }
        }
    }

    pub fn play_animation_force(&mut self, animation: &str) {
        if let Some((index, length)) = self.animations.get(animation) {
            self.playing_animation = Some(PlayingAnimation {
                index: *index,
                length: *length,
                frame: 0,
            });
        } else {
            #[cfg(debug_assertions)]
            panic!("unknown animation: {}", animation);
        }
    }

    pub fn play_animation_if_finished(&mut self, animation: &str) {
        if self.playing_animation.is_none() {
            self.play_animation_force(animation);
        }
    }
}

impl <E: EntityExtension<View = Self>> EntityViewController<E> for CustomEntityViewController {
    fn write_spawn_packets(entity: &Entity<E>, buffer: &mut PacketBuffer) {
        for (index, bone) in entity.view.bones.iter().enumerate() {
            let entity_id = if index == 0 {
                entity.view.root_bone
            } else {
                entity.view.bone_ids[index - 1]
            };
            
            buffer.write_packet(&play::clientbound::AddEntity {
                id: entity_id,
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
    
            let mut nbt = CachedNBT::new();
            nbt.insert_int("CustomModelData", bone.custom_model_data as i32 + entity.view.current_variant as i32);
    
            let mut metadata = ItemDisplayMetadata::default();
            metadata.set_item_stack(ProtocolItemStack {
                item: graphite_mc_constants::item::Item::Stick as i32,
                count: 1,
                nbt: Cow::Owned(nbt),
            });

            metadata.set_translation(bone.position.into());
            metadata.set_left_rotation(bone.rotation.into());
            metadata.set_pos_rot_interpolation_duration(2);
            metadata.set_transformation_interpolation_duration(2);
    
            metadata.write_metadata_changes_packet(entity_id, buffer).unwrap();
        }

        buffer.write_packet(&play::clientbound::AddEntity {
            id: entity.view.interaction_id,
            uuid: rand::random(),
            entity_type: graphite_mc_constants::entity::Entity::Slime as i32,
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

        let mut metadata = SlimeMetadata::default();
        metadata.set_id_size(entity.view.slime_size);
        metadata.set_shared_flags(metadata.shared_flags | (1 << 5));
        metadata.write_metadata_changes_packet(entity.view.interaction_id, buffer).unwrap();

        let mut passengers = vec![entity.view.interaction_id];
        for bone_id in &entity.view.bone_ids {
            passengers.push(*bone_id);
        }

        buffer.write_packet(&SetPassengers {
            entity_id: entity.view.root_bone,
            passengers: Cow::Owned(passengers),
        }).unwrap();
    }

    fn write_despawn_packets(entity: &Entity<E>, despawn_list: &mut Vec<i32>, _: &mut PacketBuffer) {
        despawn_list.push(entity.view.interaction_id);
        despawn_list.push(entity.view.root_bone);
        despawn_list.extend(&entity.view.bone_ids);
    }

    fn update_position(entity: &mut Entity<E>) {
        let size = entity.view.bones.len();

        if let Some(playing_animation) = &entity.view.playing_animation {
            let animation_index = playing_animation.index;
            let animation_frame = playing_animation.frame;

            for index in 0..size {
                let bone = &entity.view.bones[index];
                let animation = &bone.animations[animation_index];
    
                let entity_id = if index == 0 {
                    entity.view.root_bone
                } else {
                    entity.view.bone_ids[index - 1]
                };
    
                let mut rotation_from_parent = bone.rotation_from_parent;
                let mut offset_from_parent = bone.offset_from_parent;
    
                if let Some(frame) = animation.frames.get(animation_frame) {
                    if let Some(rotation) = frame.rotation {
                        let rot_x = (rotation.0 + bone.raw_rotation.0).to_radians();
                        let rot_y = (rotation.1 + bone.raw_rotation.1).to_radians();
                        let rot_z = (rotation.2 + bone.raw_rotation.2).to_radians();
                        let rotation = glam::Quat::from_euler(glam::EulerRot::ZYX,
                            rot_z, -rot_y, -rot_x);
                        rotation_from_parent = rotation;
                    } else {
                        let mut before = None;
                        let mut before_delta = 0;
                        let mut after = None;
                        let mut after_delta = 0;
    
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
    
                                let rot_x = (rotation.x + bone.raw_rotation.0).to_radians();
                                let rot_y = (rotation.y + bone.raw_rotation.1).to_radians();
                                let rot_z = (rotation.z + bone.raw_rotation.2).to_radians();
                                let rotation = glam::Quat::from_euler(glam::EulerRot::ZYX,
                                    rot_z, -rot_y, -rot_x);
                                rotation_from_parent = rotation;
                            }
                        }
                    }
                    if let Some(position) = frame.position {
                        offset_from_parent += glam::Vec3::from(position);
                    } else {
                        let mut before = None;
                        let mut before_delta = 0;
                        let mut after = None;
                        let mut after_delta = 0;
    
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
    
                                offset_from_parent += glam::Vec3::from(position);
                            }
                        }
                    }
                }
    
                let new_rotation;
                let new_position;
    
                if let Some(parent_index) = bone.parent {
                    let parent = entity.view.bones.get(parent_index).unwrap();
                    if animation.global_rotation {
                        new_rotation = rotation_from_parent;
                    } else {
                        new_rotation = parent.rotation.mul_quat(rotation_from_parent);
                    }
                    let rotated_offset = parent.rotation.mul_vec3(offset_from_parent);
                    new_position = parent.position + rotated_offset;
                } else {
                    new_rotation = rotation_from_parent;
                    new_position = offset_from_parent;
                }
    
                let bone = &mut entity.view.bones[index];
                bone.position = new_position;
                bone.rotation = new_rotation;
    
                let mut metadata = ItemDisplayMetadata::default();
    
                metadata.set_translation(bone.position.into());
                metadata.set_left_rotation(bone.rotation.into());
                metadata.set_transformation_interpolation_start_delta_ticks(0);
    
                // todo: combine with thing below
                entity.write_viewable(|buffer| {
                    metadata.write_metadata_changes_packet(entity_id, buffer).unwrap();
                });
            }

            // Update frame
            let playing_animation = entity.view.playing_animation.as_mut().unwrap();
            playing_animation.frame += 1;
            if playing_animation.frame > playing_animation.length {
                entity.view.playing_animation = None;
            }
        }

        if entity.view.current_variant != entity.view.last_variant {
            entity.view.last_variant = entity.view.current_variant;

            let size = entity.view.bones.len();

            for index in 0..size {
                let bone = &entity.view.bones[index];

                let entity_id = if index == 0 {
                    entity.view.root_bone
                } else {
                    entity.view.bone_ids[index - 1]
                };
        
                let mut nbt = CachedNBT::new();
                nbt.insert_int("CustomModelData", bone.custom_model_data as i32 + entity.view.current_variant as i32);
        
                let mut metadata = ItemDisplayMetadata::default();
                metadata.set_item_stack(ProtocolItemStack {
                    item: graphite_mc_constants::item::Item::Stick as i32,
                    count: 1,
                    nbt: Cow::Owned(nbt),
                });
        
                entity.write_viewable(|buffer| {
                    metadata.write_metadata_changes_packet(entity_id, buffer).unwrap();
                });
            }
        }

        let new_rotation = (
            ByteRotation::from_f32(entity.rotation.x as f32),
            ByteRotation::from_f32(entity.rotation.y as f32)
        );
    
        if entity.view.old_rotation != new_rotation {
            let len = entity.view.bone_ids.len();
            for i in 0..len {
                let move_packet = MoveEntityRot {
                    entity_id: *entity.view.bone_ids.get(i).unwrap(),
                    yaw: entity.rotation.y as _,
                    pitch: entity.rotation.x as _,
                    on_ground: false,
                };
                let _ = entity.add_viewable_packet(&move_packet);
            }
        }

        let Some(synced_position) = entity.view.synced_position else {
            // Force teleport for first tick
            let teleport_packet = TeleportEntity {
                entity_id: entity.view.root_bone,
                x: entity.position.x as _,
                y: entity.position.y as _,
                z: entity.position.z as _,
                yaw: entity.rotation.y as _,
                pitch: entity.rotation.x as _,
                on_ground: false,
            };
            let _ = entity.add_viewable_packet(&teleport_packet);
    
            entity.view.old_rotation = new_rotation;
            entity.view.teleport_time = 0;
            entity.view.synced_position = Some(entity.position);
            return;
        };
    
        let delta = entity.position - synced_position;
        let quantized = delta * 4096.0;
    
        entity.view.teleport_time = entity.view.teleport_time.wrapping_add(1);
        
        if quantized.abs().max_element() < 1.0 {
            if entity.view.old_rotation != new_rotation {
                let move_packet = MoveEntityRot {
                    entity_id: entity.view.root_bone,
                    yaw: entity.rotation.y as _,
                    pitch: entity.rotation.x as _,
                    on_ground: false,
                };
                let _ = entity.add_viewable_packet(&move_packet);
    
                entity.view.old_rotation = new_rotation;
            }
            return;
        }
    
        if quantized.min_element() <= i16::MIN as f64 || quantized.max_element() >= i16::MAX as f64 || entity.view.teleport_time > 20 {
            // Force teleport due to large distance or 20 ticks since last teleport
            let teleport_packet = TeleportEntity {
                entity_id: entity.view.root_bone,
                x: entity.position.x as _,
                y: entity.position.y as _,
                z: entity.position.z as _,
                yaw: entity.rotation.y as _,
                pitch: entity.rotation.x as _,
                on_ground: false,
            };
            let _ = entity.add_viewable_packet(&teleport_packet);
    
            entity.view.old_rotation = new_rotation;
            entity.view.teleport_time = 0;
            entity.view.synced_position = Some(entity.position);
        } else {
            // Relative move
            let quantized = quantized.as_i16vec3();
            entity.view.synced_position = Some(synced_position + quantized.as_dvec3() * 4096.0);
    
            if entity.view.old_rotation != new_rotation {
                let move_packet = MoveEntityPosRot {
                    entity_id: entity.view.root_bone,
                    delta_x: quantized.x,
                    delta_y: quantized.y,
                    delta_z: quantized.z,
                    yaw: entity.rotation.y as _,
                    pitch: entity.rotation.x as _,
                    on_ground: false,
                };
                let _ = entity.add_viewable_packet(&move_packet);
    
                entity.view.old_rotation = new_rotation;
            } else {
                let move_packet = MoveEntityPos {
                    entity_id: entity.view.root_bone,
                    delta_x: quantized.x,
                    delta_y: quantized.y,
                    delta_z: quantized.z,
                    on_ground: false,
                };
                let _ = entity.add_viewable_packet(&move_packet);
            }
        }
    }

    fn get_exposed_ids(entity: &mut Entity<E>) -> Vec<i32> {
        vec![entity.view.interaction_id]
    }

    fn get_main_exposed_id(entity: &mut Entity<E>) -> Option<i32> {
        Some(entity.view.interaction_id)
    }
}

struct Bone {
    custom_model_data: isize,
    parent: Option<usize>,
    rotation: glam::Quat,
    position: glam::Vec3,
    raw_rotation: (f32, f32, f32),
    rotation_from_parent: glam::Quat,
    offset_from_parent: glam::Vec3,
    animations: Vec<BoneAnimation>
}

#[derive(Deserialize)]
pub struct CustomEntityDefinition {
    bones: Vec<CustomEntityBone>,
    variants: Vec<String>,
    animations: HashMap<String, (usize, usize)>
}

impl CustomEntityDefinition {
    pub fn from_string(str: &str) -> Option<Self> {
        serde_json::from_str(str).ok()
    }
}

#[derive(Deserialize)]
pub struct CustomEntityBone {
    custom_model_data: isize,
    parent: Option<usize>,
    rotation: (f32, f32, f32),
    offset: (f32, f32, f32),
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
    position: Option<(f32, f32, f32)>
}