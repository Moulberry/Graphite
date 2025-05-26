use std::borrow::Cow;

use glam::{DVec2, DVec3};
use graphite_binary::nbt::EncodedNBT;
use graphite_mc_constants::{builtin::Attribute, entity::{AcaciaBoatMetadata, Entity, EntityAndMetadata, ItemDisplayMetadata}, item::Item, types::{EquipmentSlot, Pose}};
use graphite_mc_protocol::{play::{self, clientbound::{AddEntity, AttributeEntry, EntityPositionSync, MoveEntityPos, MoveEntityPosRot, MoveEntityRot, PlayerInfoAction, PlayerInfoEntry, RotateHead, SetEquipment, SetPassengers, UpdateAttributes}}, types::{data_component::ItemModel, text::TextComponent, ByteRotation, GameProfile, GameProfileProperty, ItemStack}, IdentifiedPacket};
use graphite_network::PacketBuffer;

use crate::{types::AABB, world::{chunk::Chunk, RemoteEntityId}};

pub struct RemoteEntity {
    pub(crate) id: Option<RemoteEntityId>,
    pub ecs_id: hecs::Entity,
    uuid: u128,
    network_id: i32,
    riding_id: Option<i32>,

    entity: EntityAndMetadata,
    is_living_entity: bool,
    attributes: Vec<AttributeEntry<'static>>,
    scale: f32,

    profile: Option<GameProfile<'static>>,

    // equipment: [ItemStack; 6],

    lerp_ticks: usize,
    client_position: DVec3,

    synced_position: Option<DVec3>,
    old_rotation: (u8, u8),
    teleport_time: usize,
}

impl RemoteEntity {
    pub fn new(entity: EntityAndMetadata) -> Self {
        Self::new_with_attributes(entity, Vec::new())
    }

    pub fn client_position(&self) -> DVec3 {
        self.client_position
    }

    pub fn new_with_attributes(entity: EntityAndMetadata, attributes: Vec<AttributeEntry<'static>>) -> Self {
        let mut scale = 1.0;

        for attribute in &attributes {
            if attribute.id == Attribute::Scale {
                scale = attribute.value;
            }
        }

        let uuid: u128 = rand::random();
        let uuid = (uuid & !0xF0000000000000000000) | 0x30000000000000000000; // set version to 3

        let is_living_entity = entity.entity().get_properties().is_living_entity;

        Self {
            id: None,
            ecs_id: hecs::Entity::DANGLING,
            uuid,
            network_id: crate::entity::next_entity_id(),
            riding_id: None,
            entity,
            is_living_entity,
            attributes,
            scale: scale as f32,
            profile: None,
            lerp_ticks: 0,
            client_position: DVec3::ZERO,
            synced_position: None,
            old_rotation: (0, 0),
            teleport_time: 0,
        }
    }

    pub fn set_profile(&mut self, mut profile: GameProfile<'static>) {
        profile.uuid = self.uuid;
        self.profile = Some(profile);
    }

    pub fn make_riding(mut self) -> Self {
        self.riding_id = Some(crate::entity::next_entity_id());
        self
    }

    pub fn network_id(&self) -> i32 {
        self.network_id
    }

    pub fn spawn(&self, buffer: &mut PacketBuffer, position: DVec3, yaw: f64, pitch: f64) {
        if let Some(profile) = self.profile.as_ref() {
            play::clientbound::PlayerInfoUpdate {
                actions: PlayerInfoAction::AddPlayer | PlayerInfoAction::UpdateListed | PlayerInfoAction::UpdateDisplayName,
                entries: vec![
                    PlayerInfoEntry {
                        profile: profile.clone(),
                        listed: false,
                        latency: 0,
                        gamemode: 0,
                        display_name: None
                    }
                ],
            }.write_packet(buffer);
        }

        AddEntity {
            id: self.network_id,
            uuid: self.uuid,
            entity_type: self.entity.entity() as i32,
            x: position.x,
            y: position.y,
            z: position.z,
            pitch: pitch as f32,
            yaw: yaw as f32,
            head_yaw: yaw as f32,
            ..Default::default()
        }.write_packet(buffer);

        play::clientbound::SetEntityData::write_non_default(&self.entity, self.network_id, buffer);

        if !self.attributes.is_empty() {
            UpdateAttributes {
                entity_id: self.network_id,
                attribute: Cow::Borrowed(&self.attributes),
            }.write_packet(buffer);
        }

        if let Some(riding_id) = self.riding_id {
            AddEntity {
                id: riding_id,
                uuid: rand::random(),
                entity_type: Entity::ItemDisplay as i32,
                x: position.x,
                y: position.y,
                z: position.z,
                pitch: pitch as f32,
                yaw: yaw as f32,
                head_yaw: yaw as f32,
                ..Default::default()
            }.write_packet(buffer);

            SetPassengers {
                entity_id: riding_id,
                passengers: Cow::Borrowed(&[self.network_id]),
            }.write_packet(buffer);

            EntityPositionSync {
                entity_id: self.network_id,
                x: position.x as _,
                y: position.y as _,
                z: position.z as _,
                x_vel: 0.0,
                y_vel: 0.0,
                z_vel: 0.0,
                yaw: yaw as _,
                pitch: pitch as _,
                on_ground: false,
            }.write_packet(buffer);
        }
    }

    pub fn despawn(&self, despawn_vec: &mut Vec<i32>, buffer: &mut PacketBuffer) {
        despawn_vec.push(self.network_id);
        if let Some(riding_id) = self.riding_id {
            despawn_vec.push(riding_id);
        }

        if self.profile.is_some() {
            play::clientbound::PlayerInfoRemove {
                players: vec![self.uuid],
            }.write_packet(buffer);
        }
    }

    pub fn tick(&mut self, chunk: &mut Chunk, position: DVec3, yaw: f64, pitch: f64) {
        self.update_position(&mut chunk.entity_viewable, position, yaw, pitch, self.is_living_entity);

        if self.lerp_ticks <= 0 {
            self.client_position = self.synced_position.unwrap();
        } else {
            self.client_position = self.client_position.lerp(self.synced_position.unwrap(), 1.0 / self.lerp_ticks as f64);
            self.lerp_ticks -= 1;
        }

        if matches!(self.entity, EntityAndMetadata::Shulker(_) | EntityAndMetadata::OakBoat(_)) {
            if let Some(aabb) = self.create_aabb() {
                chunk.pending_solid_entity_aabbs.push(aabb);
            }
        } else if self.is_living_entity && !matches!(self.entity, EntityAndMetadata::ArmorStand(_) | EntityAndMetadata::Bat(_) | EntityAndMetadata::Parrot(_)) {
            // LivingEntities except ArmorStand, Bat, Parrot can push the player
            if let Some(aabb) = self.create_aabb() {
                chunk.pending_soft_entity_aabbs.push(aabb);
            }
        }
    }

    pub fn create_aabb(&self) -> Option<AABB> {
        let scale = self.scale;
        let aabb = match self.entity {
            EntityAndMetadata::AreaEffectCloud(ref metadata) => {
                AABB::centered_bottom(metadata.radius as f64 * 2.0, 0.5)
            },
            EntityAndMetadata::Interaction(ref metadata) => {
                AABB::centered_bottom(metadata.width as f64, metadata.height as f64)
            },
            EntityAndMetadata::Slime(ref metadata) => {
                let dimensions = self.entity.entity().get_default_dimensions(metadata.pose);
                let scaled = dimensions.scale(metadata.id_size as f32 * scale);
                AABB::centered_bottom(scaled.width as f64, scaled.height as f64)
            },
            _ => {
                // note: many entities have custom behaviour here for when they are babies
                // this behaviour hasn't been implemented
                let pose = self.entity.get_pose();
                let dimensions = self.entity.entity().get_default_dimensions(pose);
                let scaled = dimensions.scale(scale);
                AABB::centered_bottom(scaled.width as f64, scaled.height as f64)
            }
        };
        let aabb = aabb.translate(self.client_position);
        Some(aabb)
    }

    fn update_position(&mut self, packet_buffer: &mut PacketBuffer, position: DVec3, yaw: f64, pitch: f64, send_head_rotation: bool) {
        let root_id = if let Some(riding_id) = self.riding_id {
            riding_id
        } else {
            self.network_id
        };

        let new_rotation = (
            ByteRotation::from_f32(pitch as f32),
            ByteRotation::from_f32(yaw as f32)
        );
        let Some(synced_position) = self.synced_position.as_mut() else {
            // Force teleport for first tick
            let teleport_packet = EntityPositionSync {
                entity_id: root_id,
                x: position.x as _,
                y: position.y as _,
                z: position.z as _,
                x_vel: 0.0,
                y_vel: 0.0,
                z_vel: 0.0,
                yaw: yaw as _,
                pitch: pitch as _,
                on_ground: false,
            };
            teleport_packet.write_packet(packet_buffer);
            
            self.old_rotation = new_rotation;
            self.teleport_time = 0;
            self.synced_position = Some(position);
            self.client_position = position;
            self.lerp_ticks = self.entity.entity().get_properties().interpolation_duration;
            return;
        };
        let delta = position - *synced_position;
        let quantized = delta * 4096.0;
        if self.teleport_time < 400 {
            self.teleport_time += 1;
        }

        if self.old_rotation != new_rotation && root_id != self.network_id {
            MoveEntityRot {
                entity_id: self.network_id,
                yaw: yaw as _,
                pitch: pitch as _,
                on_ground: false,
            }.write_packet(packet_buffer);
            if send_head_rotation {
                RotateHead {
                    entity_id: self.network_id,
                    head_yaw: yaw as _,
                }.write_packet(packet_buffer);
            }
        }

        if quantized.abs().max_element() < 1.0 {
            if self.old_rotation != new_rotation {
                MoveEntityRot {
                    entity_id: root_id,
                    yaw: yaw as _,
                    pitch: pitch as _,
                    on_ground: false,
                }.write_packet(packet_buffer);
                if send_head_rotation && root_id == self.network_id {
                    RotateHead {
                        entity_id: root_id,
                        head_yaw: yaw as _,
                    }.write_packet(packet_buffer);
                }
            
                self.old_rotation = new_rotation;
            }
            return;
        }   
            
        if quantized.min_element() <= i16::MIN as f64 || quantized.max_element() >= i16::MAX as f64 || self.teleport_time >= 400 {
            // Force teleport due to large distance or 20 seconds since last teleport
            EntityPositionSync {
                entity_id: root_id,
                x: position.x as _,
                y: position.y as _,
                z: position.z as _,
                x_vel: 0.0,
                y_vel: 0.0,
                z_vel: 0.0,
                yaw: yaw as _,
                pitch: pitch as _,
                on_ground: false,
            }.write_packet(packet_buffer);
            if send_head_rotation && root_id == self.network_id {
                RotateHead {
                    entity_id: root_id,
                    head_yaw: yaw as _,
                }.write_packet(packet_buffer);
            }
            
            self.old_rotation = new_rotation;
            self.teleport_time = 0;
            *synced_position = position;
            self.lerp_ticks = self.entity.entity().get_properties().interpolation_duration;
        } else {
            // Relative move
            let quantized = quantized.as_i16vec3();
            *synced_position += quantized.as_dvec3() / 4096.0;
            self.lerp_ticks = self.entity.entity().get_properties().interpolation_duration;
            
            if self.old_rotation != new_rotation {
                MoveEntityPosRot {
                    entity_id: root_id,
                    delta_x: quantized.x,
                    delta_y: quantized.y,
                    delta_z: quantized.z,
                    yaw: yaw as _,
                    pitch: pitch as _,
                    on_ground: false,
                }.write_packet(packet_buffer);
                if send_head_rotation && root_id == self.network_id {
                    RotateHead {
                        entity_id: root_id,
                        head_yaw: yaw as _,
                    }.write_packet(packet_buffer);
                }
            
                self.old_rotation = new_rotation;
            } else {
                MoveEntityPos {
                    entity_id: root_id,
                    delta_x: quantized.x,
                    delta_y: quantized.y,
                    delta_z: quantized.z,
                    on_ground: false,
                }.write_packet(packet_buffer);
            }
        }
    }
    
    pub fn is_attackable(&self) -> bool {
        match self.entity {
            EntityAndMetadata::Arrow(_) => false,
            EntityAndMetadata::ExperienceOrb(_) => false,
            EntityAndMetadata::EyeOfEnder(_) => false,
            EntityAndMetadata::FallingBlock(_) => false,
            EntityAndMetadata::FireworkRocket(_) => false,
            EntityAndMetadata::Item(_) => false,
            _ => true
        }
    }

    pub fn skip_attack_interaction(&self) -> bool {
        match self.entity {
            // BlockAttachedEntity
            EntityAndMetadata::ItemFrame(_) | EntityAndMetadata::GlowItemFrame(_) | EntityAndMetadata::LeashKnot(_) | EntityAndMetadata::Painting(_) => {
                true
            },
            EntityAndMetadata::Interaction(ref interaction) => {
                !interaction.response
            },
            _ => false
        }
    }

    pub fn can_hurt_client(&self) -> bool {
        match self.entity {
            // BlockAttachedEntity
            EntityAndMetadata::ItemFrame(_) | EntityAndMetadata::GlowItemFrame(_) | EntityAndMetadata::LeashKnot(_) | EntityAndMetadata::Painting(_) => {
                true
            },
            EntityAndMetadata::EndCrystal(_) => true,
            EntityAndMetadata::ExperienceOrb(_) => true,
            EntityAndMetadata::Item(_) => true,
            EntityAndMetadata::Player(_) => true,
            EntityAndMetadata::ShulkerBullet(_) => true,
            // todo: vehicle entity (there are a lot)

            _ => false
        }
    }
}