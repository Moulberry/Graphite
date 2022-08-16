use std::{mem::ManuallyDrop, ops::Range};

use anyhow::bail;
use binary::slice_serialization::SliceSerializable;
use minecraft_constants::entity::{Metadata, PlayerMetadata};
use net::{
    network_buffer::WriteBuffer,
    packet_helper::{self, PacketReadResult},
};
use parry3d::{
    bounding_volume::AABB,
    math::{Point, Real, Vector},
    query::Ray,
};
use protocol::{
    play::{
        client::PacketHandler,
        server::{
            self, AddPlayer, BlockChangedAck, BlockDestruction, LevelEvent, LevelEventType,
            PlayerInfo, PlayerInfoAddPlayer, RemoveEntities, RotateHead,
            SetEquipment, TeleportEntity, MoveEntityPosRot, MoveEntityPos, MoveEntityRot,
        },
    },
    types::{BlockPosition, EquipmentSlot, GameProfile, Hand},
    IdentifiedPacket,
};
use queues::Buffer;
use rand::RngCore;
use sticky::Unsticky;
use text_component::TextComponent;

use crate::{
    entity::position::{Position, Vec3f, Coordinate},
    gamemode::Abilities,
    inventory::inventory_handler::{InventoryHandler, InventorySlot, ItemSlot},
    universe::{EntityId, UniverseService},
    world::{
        chunk::BlockStorage, ChunkViewPosition, TickPhase, TickPhaseInner, World, WorldService,
    },
};

use super::{
    interaction::{Interaction, InteractionState},
    player_connection::AbstractConnectionReference,
    player_settings::PlayerSettings,
    proto_player::ProtoPlayer,
};

// User defined player service trait

pub trait PlayerService
where
    Self: Sized + 'static,
{
    /// This will cause packets to be written immediately when packets are received
    /// If this is false, the server will instead wait for the tick
    ///
    /// Benefit: reduce latency by 50ms for 25% of players
    /// Drawback: 2x write operations which could potentially strain the server
    const FAST_PACKET_RESPONSE: bool = true;

    type UniverseServiceType: UniverseService;
    type WorldServiceType: WorldService<UniverseServiceType = Self::UniverseServiceType>;

    type InventoryHandlerType: InventoryHandler;
}

#[allow(type_alias_bounds)] // Justification: used as a shortcut to avoid monsterous type
type ConnectionReferenceType<P: PlayerService> =
    <P::UniverseServiceType as UniverseService>::ConnectionReferenceType;

// graphite player
pub struct Player<P: PlayerService> {
    world: *mut World<P::WorldServiceType>,

    pub(crate) write_buffer: WriteBuffer,
    pub(crate) viewable_self_exclusion_write_buffer: WriteBuffer,
    pub(crate) disconnected: bool,

    pub entity_id: EntityId,
    pub abilities: Abilities,
    pub metadata: PlayerMetadata,
    pub inventory: P::InventoryHandlerType,
    pub settings: PlayerSettings,
    pub profile: GameProfile,

    last_position: Position, // used to check for changes
    synced_coord: Coordinate, // used to calculate correct quantized movement
    pub(crate) client_position: Position,
    pub position: Position,
    pub on_ground: bool,

    pub selected_hotbar_slot: u8,
    last_selected_hotbar_slot: u8,

    viewable_exclusion_range: Range<usize>,
    pub(crate) chunk_view_position: ChunkViewPosition,
    pub(crate) new_chunk_view_position: ChunkViewPosition,
    pub(crate) chunk_ref: usize,
    pub(crate) teleport_id_timer: u8,
    pub(crate) waiting_teleportation_id: Buffer<i32>,
    pub(crate) ack_sequence_up_to: Option<i32>,
    pub(crate) interaction_state: InteractionState,

    pub(crate) current_keep_alive: u64,
    keep_alive_timer: u8,

    moved_into_proto: bool,
    connection: ManuallyDrop<ConnectionReferenceType<P>>,
    pub service: ManuallyDrop<P>,
}

// graphite player impl

impl<P: PlayerService> Player<P> {
    pub(crate) fn new(
        service: P,
        world: &mut World<P::WorldServiceType>,
        position: Position,
        view_position: ChunkViewPosition,
        proto_player: ProtoPlayer<P::UniverseServiceType>
    ) -> Self {
        Self {
            world,

            write_buffer: WriteBuffer::new(),
            viewable_self_exclusion_write_buffer: WriteBuffer::new(),
            disconnected: false,

            entity_id: proto_player.entity_id,
            abilities: proto_player.abilities,
            inventory: Default::default(),
            settings: PlayerSettings::new(),
            profile: proto_player.profile,
            metadata: Default::default(),

            last_position: position,
            synced_coord: position.coord,
            client_position: position,
            position,
            on_ground: false,

            selected_hotbar_slot: 0,
            last_selected_hotbar_slot: 0,

            viewable_exclusion_range: 0..0,
            chunk_view_position: view_position,
            new_chunk_view_position: view_position,
            chunk_ref: usize::MAX,
            teleport_id_timer: 0,
            waiting_teleportation_id: Buffer::new(20),
            ack_sequence_up_to: None,
            interaction_state: Default::default(),

            current_keep_alive: 0,
            keep_alive_timer: 0,

            moved_into_proto: false,
            connection: ManuallyDrop::new(proto_player.connection),
            service: ManuallyDrop::new(service),
        }
    }

    pub fn get_world<'a, 'b>(&'a self) -> &'b World<P::WorldServiceType> {
        unsafe { self.world.as_ref().unwrap() }
    }

    pub fn get_world_mut<'a, 'b>(&'a mut self) -> &'b mut World<P::WorldServiceType> {
        unsafe { self.world.as_mut().unwrap() }
    }

    pub(crate) fn tick(&mut self, tick_phase: TickPhase) -> anyhow::Result<()> {
        if self.disconnected {
            bail!("player has been disconnected");
        }

        if tick_phase.0 == TickPhaseInner::View {
            // Copy viewable packets
            let chunk_x = self.chunk_view_position.x as i32;
            let chunk_z = self.chunk_view_position.z as i32;

            let chunks = &self.get_world().chunks;

            // Entity viewable buffers
            let view_distance = P::WorldServiceType::ENTITY_VIEW_DISTANCE as i32;
            for x in (chunk_x - view_distance).max(0)
                ..(chunk_x + view_distance + 1).min(P::WorldServiceType::CHUNKS_X as _)
            {
                let chunks_list = &chunks[x as usize];

                for z in (chunk_z - view_distance).max(0)
                    ..(chunk_z + view_distance + 1).min(P::WorldServiceType::CHUNKS_Z as _)
                {
                    let chunk = &chunks_list[z as usize];

                    let bytes = chunk.entity_viewable_buffer.get_written();

                    if x == chunk_x && z == chunk_z {
                        self.write_buffer
                            .copy_from(&bytes[..self.viewable_exclusion_range.start]);
                        self.write_buffer
                            .copy_from(&bytes[self.viewable_exclusion_range.end..]);
                        self.viewable_exclusion_range = 0..0;
                    } else {
                        self.write_buffer.copy_from(bytes);
                    }
                }
            }

            // Block viewable buffers
            let view_distance = P::WorldServiceType::CHUNK_VIEW_DISTANCE as i32;
            for x in (chunk_x - view_distance).max(0)
                ..(chunk_x + view_distance + 1).min(P::WorldServiceType::CHUNKS_X as _)
            {
                for z in (chunk_z - view_distance).max(0)
                    ..(chunk_z + view_distance + 1).min(P::WorldServiceType::CHUNKS_Z as _)
                {
                    let chunk = &self.get_world().chunks[x as usize][z as usize];
                    self.write_buffer
                        .copy_from(chunk.block_viewable_buffer.get_written());
                }
            }

            self.chunk_view_position = self.new_chunk_view_position;

            // Write packets from buffer
            if !self.write_buffer.is_empty() {
                // Write bytes into player connection
                self.connection.write_bytes(self.write_buffer.get_written());

                // Reset the write buffer
                self.write_buffer.reset();
            }
            self.write_buffer.tick_and_maybe_shrink();

            // Return early -- code after here is for TickPhase::Update
            return Ok(());
        }

        // Check teleport timer
        if self.teleport_id_timer > 0 {
            self.teleport_id_timer += 1;

            if self.teleport_id_timer >= 20 {
                bail!("player sent incorrect teleport id and failed to rectify within time limit");
            }
        }

        // Send block change ack
        match self.ack_sequence_up_to {
            Some(sequence) => {
                self.write_packet(&BlockChangedAck { sequence });
                self.ack_sequence_up_to = None;
            }
            None => (),
        }

        // Send keep alive timer
        self.keep_alive_timer = self.keep_alive_timer.wrapping_add(1);
        if self.keep_alive_timer == 0 {
            if self.current_keep_alive != 0 {
                bail!("client hasn't responded with keep alive")
            }
            self.current_keep_alive = rand::thread_rng().next_u64();

            self.write_packet(&server::KeepAlive {
                id: self.current_keep_alive,
            });
        }

        // Update selected hotbar slot
        let selected_hotbar_slot_changed =
            self.last_selected_hotbar_slot != self.selected_hotbar_slot;
        if selected_hotbar_slot_changed {
            self.last_selected_hotbar_slot = self.selected_hotbar_slot;

            // If we were using an item, abort the use
            if self.interaction_state.using_hand == Some(Hand::Main) {
                let interaction = self.interaction_state.try_abort_use(false).unwrap();
                self.fire_interaction(interaction);
            }
        }

        // Start item continuous usage
        if let Some(hand) = self.interaction_state.get_used_hand() {
            let inventory_slot = match hand {
                Hand::Main => InventorySlot::Hotbar(self.selected_hotbar_slot as _),
                Hand::Off => InventorySlot::OffHand,
            };

            let slot = self.inventory.get(inventory_slot)?;
            match slot {
                ItemSlot::Empty => (),
                ItemSlot::Filled(item) => {
                    if item.properties.use_duration > 0 {
                        self.interaction_state
                            .start_using(item.properties.use_duration as _, hand);
                    }
                }
            }
        }

        // Update interaction state
        for interaction in self.interaction_state.update() {
            self.fire_interaction(interaction);
        }

        // Equipment changes
        let mut equipment_changes = vec![];

        // Add MainHand if hotbar slot has changed
        if selected_hotbar_slot_changed {
            let slot = InventorySlot::Hotbar(self.selected_hotbar_slot as _);
            let held_item = self
                .inventory
                .get(slot)
                .expect("self.selected_hotbar_slot between 0..9");

            equipment_changes.push((EquipmentSlot::MainHand, held_item.into()));
        }

        // Add other equipment
        if self.inventory.is_any_changed() {
            // Update MainHand
            let slot = InventorySlot::Hotbar(self.selected_hotbar_slot as _);
            if self.inventory.is_changed(slot).unwrap() {
                // Abort item usage for main hand
                if self.interaction_state.using_hand == Some(Hand::Main) {
                    let interaction = self.interaction_state.try_abort_use(false).unwrap();
                    self.fire_interaction(interaction);
                }

                // Update equipment for MainHand, if needed
                if !selected_hotbar_slot_changed {
                    let itemslot = self.inventory.get(slot).unwrap();
                    equipment_changes.push((EquipmentSlot::MainHand, itemslot.into()));
                }
            }

            let mut write_equipment_changes =
                |inventory: InventorySlot, equipment: EquipmentSlot| {
                    if self.inventory.is_changed(inventory).unwrap() {
                        let itemslot = self.inventory.get(inventory).unwrap();
                        equipment_changes.push((equipment, itemslot.into()));
                        true
                    } else {
                        false
                    }
                };

            // Update armor
            write_equipment_changes(InventorySlot::Feet, EquipmentSlot::Feet);
            write_equipment_changes(InventorySlot::Legs, EquipmentSlot::Legs);
            write_equipment_changes(InventorySlot::Chest, EquipmentSlot::Chest);
            write_equipment_changes(InventorySlot::Head, EquipmentSlot::Head);

            // Update OffHand
            if write_equipment_changes(InventorySlot::OffHand, EquipmentSlot::OffHand) {
                // Abort item usage for off hand
                if self.interaction_state.using_hand == Some(Hand::Off) {
                    let interaction = self.interaction_state.try_abort_use(false).unwrap();
                    self.fire_interaction(interaction);
                }
            }
        }

        // Write equipment changes
        if !equipment_changes.is_empty() {
            self.write_viewable_packet(
                &SetEquipment {
                    entity_id: self.entity_id.as_i32(),
                    equipment: equipment_changes,
                },
                true,
            );
        }

        // Write inventory packets
        self.inventory.write_changes(&mut self.write_buffer)?;

        // Write abilities packets (note: this must come after equipment changes)
        Abilities::write_changes(self);

        // Write metadata packets
        let write_size = self.metadata.get_write_size();
        if write_size > 0 {
            println!("writing metadata!");

            let chunk = &mut self.get_world_mut().chunks[self.chunk_view_position.x as usize]
                [self.chunk_view_position.z as usize];

            packet_helper::write_metadata_packet(
                &mut chunk.entity_viewable_buffer,
                server::PacketId::SetEntityData as _,
                self.entity_id.as_i32(),
                &mut self.metadata,
            )?;
        }

        // Update position
        if self.position != self.last_position {
            self.handle_movement(self.position, true)?;
        } else {
            // todo: check for moving too fast
            self.handle_movement(self.client_position, false)?;
        }

        // Write packets from viewable self-exclusion
        // These packets are seen by those in render distance of this player,
        // but *NOT* this player. This is used for eg. movement
        if !self.viewable_self_exclusion_write_buffer.is_empty() {
            let chunk = &mut self.get_world_mut().chunks[self.chunk_view_position.x as usize]
                [self.chunk_view_position.z as usize];
            let write_to = &mut chunk.entity_viewable_buffer;

            // Copy bytes into viewable buffer
            let start = write_to.len();
            write_to.copy_from(self.viewable_self_exclusion_write_buffer.get_written());
            let end = write_to.len();

            // Set exclusion range
            self.viewable_exclusion_range = start..end;

            // Reset the write buffer
            self.viewable_self_exclusion_write_buffer.reset();
        }
        self.viewable_self_exclusion_write_buffer
            .tick_and_maybe_shrink();

        Ok(())
    }

    fn handle_movement(&mut self, to: Position, inform_client: bool) -> anyhow::Result<()> {
        let distance_sq = to.distance_sq(self.last_position);
        let rot_changed = to.rot.is_diff_u8(self.last_position.rot);
        let coord_changed = distance_sq > 0.0001;

        if coord_changed {
            if distance_sq < 8.0*8.0 {
                let delta_x = to.coord.x - self.synced_coord.x;
                let delta_y = to.coord.y - self.synced_coord.y;
                let delta_z = to.coord.z - self.synced_coord.z;

                let quantized_x = (delta_x * 4096.0) as i16;
                let quantized_y = (delta_y * 4096.0) as i16;
                let quantized_z = (delta_z * 4096.0) as i16;

                self.synced_coord.x += quantized_x as f32 / 4096.0;
                self.synced_coord.y += quantized_y as f32 / 4096.0;
                self.synced_coord.z += quantized_z as f32 / 4096.0;

                if rot_changed {
                    // Relative Move & Rotate
                    let move_packet = MoveEntityPosRot {
                        entity_id: self.entity_id.as_i32(),
                        delta_x: quantized_x,
                        delta_y: quantized_y,
                        delta_z: quantized_z,
                        yaw: to.rot.yaw,
                        pitch: to.rot.pitch,
                        on_ground: self.on_ground,
                    };
                    self.write_viewable_packet(&move_packet, true);
                    
                    // Rotate head
                    let rotate_head = RotateHead {
                        entity_id: self.entity_id.as_i32(),
                        head_yaw: to.rot.yaw,
                    };
                    self.write_viewable_packet(&rotate_head, true);
                } else {
                    // todo: switch to using MoveEntityPos when MC-255263 is fixed

                    // Relative Move
                    let move_packet = MoveEntityPosRot {
                        entity_id: self.entity_id.as_i32(),
                        delta_x: quantized_x,
                        delta_y: quantized_y,
                        delta_z: quantized_z,
                        yaw: to.rot.yaw,
                        pitch: to.rot.pitch,
                        on_ground: self.on_ground,
                    };
                    self.write_viewable_packet(&move_packet, true);
                }
            } else {
                self.synced_coord = to.coord;

                // Teleport
                let teleport_packet = TeleportEntity {
                    entity_id: self.entity_id.as_i32(),
                    x: to.coord.x as _,
                    y: to.coord.y as _,
                    z: to.coord.z as _,
                    yaw: to.rot.yaw,
                    pitch: to.rot.pitch,
                    on_ground: self.on_ground,
                };
                self.write_viewable_packet(&teleport_packet, true);

                if rot_changed {
                    // Rotate head
                    let rotate_head = RotateHead {
                        entity_id: self.entity_id.as_i32(),
                        head_yaw: to.rot.yaw,
                    };
                    self.write_viewable_packet(&rotate_head, true);
                }
            }

            self.get_world_mut().update_view_position(self, to)?;
        } else if rot_changed {
            // todo: use MoveEntityRot when MC-255263 is fixed

            // Teleport
            let teleport_packet = TeleportEntity {
                entity_id: self.entity_id.as_i32(),
                x: to.coord.x as _,
                y: to.coord.y as _,
                z: to.coord.z as _,
                yaw: to.rot.yaw,
                pitch: to.rot.pitch,
                on_ground: self.on_ground,
            };
            self.write_viewable_packet(&teleport_packet, true);

            // Rotate head
            let rotate_head = RotateHead {
                entity_id: self.entity_id.as_i32(),
                head_yaw: to.rot.yaw,
            };
            self.write_viewable_packet(&rotate_head, true);
        } else {
            return Ok(());
        }

        if inform_client {
            // todo: use the special move
            let teleport_packet = TeleportEntity {
                entity_id: self.entity_id.as_i32(),
                x: to.coord.x as _,
                y: to.coord.y as _,
                z: to.coord.z as _,
                yaw: to.rot.yaw,
                pitch: to.rot.pitch,
                on_ground: self.on_ground,
            };
            self.write_packet(&teleport_packet);
        }

        self.position = to;
        self.last_position = to;
        self.client_position = to;

        Ok(())
    }

    pub fn clip_block_position(&self, position: BlockPosition) -> Option<(f32, f32)> {
        let aabb = AABB::new(
            Point::new(position.x as f32, position.y as f32, position.z as f32),
            Point::new(
                position.x as f32 + 1.0,
                position.y as f32 + 1.0,
                position.z as f32 + 1.0,
            ),
        );

        aabb.clip_ray_parameters(&self.get_look_ray())
    }

    pub fn get_look_ray(&self) -> Ray {
        Ray::new(
            Point::new(
                self.client_position.coord.x,
                self.client_position.coord.y + self.get_eye_height(),
                self.client_position.coord.z,
            ),
            self.get_look_vector(),
        )
    }

    pub fn get_eye_height(&self) -> f32 {
        1.62
    }

    pub fn get_look_vector(&self) -> Vector<Real> {
        let pitch_rad = self.client_position.rot.pitch.to_radians();
        let yaw_rad = -self.client_position.rot.yaw.to_radians();
        let (pitch_sin, pitch_cos) = pitch_rad.sin_cos();
        let (yaw_sin, yaw_cos) = yaw_rad.sin_cos();
        Vector::new(yaw_sin * pitch_cos, -pitch_sin, yaw_cos * pitch_cos)
    }

    fn break_block(&mut self, pos: BlockPosition) {
        if pos.x >= 0 && pos.y >= 0 && pos.z >= 0 {
            if let Some(old) = self
                .get_world_mut()
                .set_block(pos.x as _, pos.y as _, pos.z as _, 0)
            {
                self.write_viewable_packet(
                    &LevelEvent {
                        event_type: LevelEventType::ParticlesDestroyBlock,
                        pos,
                        data: old as _,
                        global: false,
                    },
                    true,
                );
            }
        }
    }

    pub fn do_default_interaction(&mut self, interaction: Interaction) {
        println!("Got interaction: {:?}", interaction);

        match interaction {
            Interaction::LeftClickBlock {
                position,
                face: _,
                instabreak,
            } => {
                if instabreak {
                    self.break_block(position);
                }
            }
            Interaction::LeftClickEntity { entity_id: _ } => {
                // todo: entity interaction
            }
            Interaction::LeftClickAir => {}

            Interaction::RightClickBlock {
                position: _,
                face: _,
                offset: _,
            } => {}
            Interaction::RightClickEntity {
                entity_id: _,
                offset: _,
            } => {
                // todo: entity interaction
            }
            Interaction::RightClickAir { hand: _ } => {}

            Interaction::ContinuousBreak {
                position,
                break_time,
                distance: _,
            } => {
                if let Some(destroy_stage) = self.get_world().get_destroy_stage(
                    position.x,
                    position.y as _,
                    position.z,
                    break_time,
                    self.get_break_speed_multiplier(),
                ) {
                    // todo: check if the stage changed, only send packet then
                    self.write_viewable_packet(
                        &BlockDestruction {
                            entity_id: self.entity_id.as_i32(),
                            location: position,
                            destroy_stage,
                        },
                        true,
                    );
                }

                // update destruction
            }
            Interaction::FinishBreak {
                position,
                break_time: _,
                distance: _,
            } => {
                self.break_block(position);
            }
            Interaction::AbortBreak {
                position,
                break_time: _,
            } => {
                self.write_viewable_packet(
                    &BlockDestruction {
                        entity_id: self.entity_id.as_i32(),
                        location: position,
                        destroy_stage: -1,
                    },
                    true,
                );
            }

            Interaction::ContinuousUse { use_time, hand } => {
                if use_time == 1 {
                    self.metadata
                        .set_living_entity_flags(self.metadata.living_entity_flags | 0x1);

                    // Set the hand that is in use
                    if hand == Hand::Off {
                        self.metadata
                            .set_living_entity_flags(self.metadata.living_entity_flags | 0x2);
                    } else {
                        self.metadata
                            .set_living_entity_flags(self.metadata.living_entity_flags & !0x2);
                    }
                }
            }
            Interaction::FinishUse {
                use_time: _,
                hand: _,
            } => {
                // todo: send finish to all players
                self.metadata
                    .set_living_entity_flags(self.metadata.living_entity_flags & !0x1);
            }
            Interaction::AbortUse {
                use_time: _,
                hand: _,
                aborted_by_client: _,
            } => {
                self.metadata
                    .set_living_entity_flags(self.metadata.living_entity_flags & !0x1);
            }
        }
    }

    pub fn get_break_speed_multiplier(&self) -> f32 {
        let speed_multiplier = 1.0;
        // todo: item bonus
        // todo: efficiency bonus
        // todo: "dig speed" aka Haste bonus
        // todo: "dig slowdown" aka Mining Fatigue
        // todo: eye in water (/5)
        // todo: not on ground (/5)

        let correct_tool_multiplier = 100.0; // set to 30 if using correct tool

        correct_tool_multiplier * speed_multiplier
    }

    pub fn send_message<T: Into<TextComponent>>(&mut self, message: T) {
        self.write_packet(&server::SystemChat {
            message: message.into().to_json(),
            overlay: false,
        })
    }

    pub fn disconnect(&mut self) {
        self.disconnected = true;
    }

    pub fn write_packet<'a, T>(&mut self, packet: &'a T)
    where
        T: SliceSerializable<'a, T> + IdentifiedPacket<server::PacketId> + 'a,
    {
        if packet_helper::write_packet(&mut self.write_buffer, packet).is_err() {
            // Packet was too big
            self.disconnect();
        }
    }

    pub fn write_viewable_packet<'a, T>(&mut self, packet: &'a T, exclude_self: bool)
    where
        T: SliceSerializable<'a, T> + IdentifiedPacket<server::PacketId> + 'a,
    {
        let write_to = if exclude_self {
            &mut self.viewable_self_exclusion_write_buffer
        } else {
            let chunk = &mut self.get_world_mut().chunks[self.chunk_view_position.x as usize]
                [self.chunk_view_position.z as usize];
            &mut chunk.entity_viewable_buffer
        };

        if packet_helper::write_packet(write_to, packet).is_err() {
            // Packet was too big
            self.disconnect();
        }
    }

    pub(crate) fn fire_interaction(&mut self, interaction: Interaction) {
        // todo: send to service
        self.do_default_interaction(interaction);
    }

    pub(crate) fn write_destroy_packet(&mut self, write_buffer: &mut WriteBuffer) {
        // Remove Entity Packet
        let remove_entity_packet = RemoveEntities {
            entities: vec![self.entity_id.as_i32()],
        };
        net::packet_helper::write_packet(write_buffer, &remove_entity_packet).unwrap();

        // Remove Player Info
        let remove_info_packet = PlayerInfo::RemovePlayer {
            uuids: vec![self.profile.uuid],
        };
        net::packet_helper::write_packet(write_buffer, &remove_info_packet).unwrap();
    }

    pub(crate) fn write_create_packet(&mut self, write_buffer: &mut WriteBuffer) {
        let packet = PlayerInfo::AddPlayer {
            values: vec![PlayerInfoAddPlayer {
                profile: self.profile.clone(),
                gamemode: self.abilities.gamemode as _, // todo: gamemode
                ping: 69,                               // todo: ping
                display_name: None,
                signature_data: None,
            }],
        };
        net::packet_helper::write_packet(write_buffer, &packet).unwrap();

        let add_player_packet = AddPlayer {
            id: self.entity_id.as_i32(),
            uuid: self.profile.uuid,
            x: self.position.coord.x as _,
            y: self.position.coord.y as _,
            z: self.position.coord.z as _,
            yaw: 0.0,
            pitch: 0.0,
        };
        net::packet_helper::write_packet(write_buffer, &add_player_packet).unwrap();

        // todo: equipment

        // todo: metadata
    }

    pub(crate) fn write_packet_bytes(&mut self, bytes: &[u8]) {
        self.write_buffer.copy_from(bytes);
    }

    pub fn handle_packets(&mut self) -> anyhow::Result<u32> {
        // Read all the bytes
        // Safety: Nothing can modify the bytes that we have read
        let mut bytes = unsafe { &*(self.connection.read_bytes() as *const _) };

        // Split, parse and handle all the received packets
        loop {
            let packet_read_result = net::packet_helper::try_read_packet(&mut bytes)?;
            match packet_read_result {
                PacketReadResult::Complete(bytes) => {
                    self.parse_and_handle(bytes)?;
                }
                PacketReadResult::Partial => break,
                PacketReadResult::Empty => break,
            }
        }

        // Send contents of write buffer if FAST_PACKET_RESPONSE is enabled
        if P::FAST_PACKET_RESPONSE {
            let to_write = self.write_buffer.get_written();
            if !to_write.is_empty() {
                self.connection.write_bytes(to_write);
            }
            self.write_buffer.reset();
        }

        // Return remaining bytes
        Ok(bytes.len() as u32)
    }

    pub fn handle_disconnect(&mut self) {
        unsafe {
            self.connection.forget();
            self.disconnect();
        }
    }
}

impl<P: PlayerService> Drop for Player<P> {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            // Safety: we are dropping the player
            unsafe {
                self.get_world_mut().remove_player_from_chunk(self);
            }

            if !self.moved_into_proto {
                unsafe {
                    ManuallyDrop::drop(&mut self.connection);
                    ManuallyDrop::drop(&mut self.service);
                }
            }
        }
    }
}

unsafe impl<P: PlayerService> Unsticky for Player<P> {
    type UnstuckType = (ProtoPlayer<P::UniverseServiceType>, P);

    fn update_pointer(&mut self, _: usize) {
        let ptr: *mut Player<P> = self;
        self.connection.update_player_pointer(ptr);

        let world = self.get_world_mut();
        let chunk = &mut world.chunks[self.chunk_view_position.x as usize]
            [self.chunk_view_position.z as usize];
        if self.chunk_ref == usize::MAX {
            chunk.create_player(self);
        } else {
            chunk.update_player_pointer(self);
        }
    }

    fn unstick(mut self) -> Self::UnstuckType {
        self.moved_into_proto = true; // Prevent calling drop on connection and service

        self.connection.clear_player_pointer();

        // Safety: `self.moved_into_proto = true` means that the following values
        // will not be dropped, so its safe to take them
        let connection = unsafe { ManuallyDrop::take(&mut self.connection) };
        let service = unsafe { ManuallyDrop::take(&mut self.service) };

        // Return the ProtoPlayer and Service as a tuple
        (
            ProtoPlayer::new(connection, self.profile.clone(), self.entity_id),
            service,
        )
    }
}
