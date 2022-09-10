use std::{mem::ManuallyDrop, ops::Range};

use anyhow::bail;
use minecraft_constants::{
    block::Block,
    entity::{Metadata, PlayerMetadata},
    item::Item,
};
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
            MoveEntityPosRot, PlayerInfo, PlayerInfoAddPlayer, RemoveEntities, RotateHead,
            SetEquipment, TeleportEntity,
        },
    },
    types::{BlockPosition, EquipmentSlot, GameProfile, Hand, Pose},
};
use queues::Buffer;
use rand::RngCore;
use sticky::Unsticky;
use text_component::TextComponent;

use crate::{
    entity::position::{Coordinate, Position, Vec3f},
    gamemode::Abilities,
    inventory::inventory_handler::{InventoryHandler, InventorySlot, ItemSlot},
    universe::{EntityId, UniverseService},
    world::{
        ChunkViewPosition, TickPhase, TickPhaseInner, World, WorldService, block_update,
    },
};

use super::{
    interaction::{Interaction, InteractionState},
    packet_buffer::PacketBuffer,
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

    pub packets: PacketBuffer,
    pub(crate) disconnected: bool,

    pub entity_id: EntityId,
    pub abilities: Abilities,
    pub metadata: PlayerMetadata,
    pub inventory: P::InventoryHandlerType,
    pub settings: PlayerSettings,
    pub profile: GameProfile,

    last_position: Position,  // used to check for changes
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
        proto_player: ProtoPlayer<P::UniverseServiceType>,
    ) -> Self {
        Self {
            world,

            packets: PacketBuffer::new(),
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
                ..(chunk_x + view_distance + 1).min(chunks.get_size_x() as _)
            {
                for z in (chunk_z - view_distance).max(0)
                    ..(chunk_z + view_distance + 1).min(chunks.get_size_z() as _)
                {
                    let chunk = chunks.get(x as usize, z as usize).expect("chunk coords in bounds");

                    let bytes = chunk.entity_viewable_buffer.get_written();

                    if x == chunk_x && z == chunk_z {
                        self.packets
                            .write_raw_packets(&bytes[..self.viewable_exclusion_range.start]);
                        self.packets
                            .write_raw_packets(&bytes[self.viewable_exclusion_range.end..]);
                        self.viewable_exclusion_range = 0..0;
                    } else {
                        self.packets.write_raw_packets(bytes);
                    }
                }
            }

            // Block viewable buffers
            let view_distance = P::WorldServiceType::CHUNK_VIEW_DISTANCE as i32;
            for x in (chunk_x - view_distance).max(0)
                ..(chunk_x + view_distance + 1).min(chunks.get_size_x() as _)
            {
                for z in (chunk_z - view_distance).max(0)
                    ..(chunk_z + view_distance + 1).min(chunks.get_size_z() as _)
                {
                    let chunk = self.get_world().chunks.get(x as usize, z as usize).expect("chunk coords in bounds");
                    self.packets
                        .write_raw_packets(chunk.block_viewable_buffer.get_written());
                }
            }

            self.chunk_view_position = self.new_chunk_view_position;

            // Write packets from buffer
            if !self.packets.write_buffer.is_empty() {
                // todo: move vec out of the write_buffer, avoid a memcpy here

                // Write bytes into player connection
                self.connection
                    .write_bytes(self.packets.write_buffer.get_written());

                // Reset the write buffer
                self.packets.write_buffer.reset();
            }
            self.packets.write_buffer.tick_and_maybe_shrink();

            // Return early -- code after here is for TickPhase::Update
            return Ok(());
        }

        // Update client synchronization (keep alive, block ack, etc.)
        self.update_client_synchronization()?;

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

        // Update pose
        self.update_pose()?;

        // Update interaction state (note: before equipment changes)
        self.update_interaction_state()?;

        // Update equipment (held items and armor)
        self.update_equipment(selected_hotbar_slot_changed);

        // Write inventory packets (note: after equipment changes)
        self.inventory
            .write_changes(&mut self.packets.write_buffer)?;

        // Write abilities packets (note: after equipment changes)
        Abilities::write_changes(self);

        // Write metadata packets
        self.update_metadata()?;

        // Update position
        if self.position != self.last_position {
            self.position.rot.fix();
            self.handle_movement(self.position, true)?;
        } else {
            // todo: check for moving too fast
            self.client_position.rot.fix();
            self.handle_movement(self.client_position, false)?;
        }

        // Write packets from viewable self-exclusion
        // These packets are seen by those in render distance of this player,
        // but *NOT* this player. This is used for eg. movement
        if !self.packets.viewable_self_exclusion_write_buffer.is_empty() {
            let chunk = self.get_world_mut().chunks
                .get_mut(self.chunk_view_position.x, self.chunk_view_position.z)
                .expect("chunk coords in bounds");
            let write_to = &mut chunk.entity_viewable_buffer;

            // Copy bytes into viewable buffer
            let start = write_to.len();
            write_to.copy_from(
                self.packets
                    .viewable_self_exclusion_write_buffer
                    .get_written(),
            );
            let end = write_to.len();

            // Set exclusion range
            self.viewable_exclusion_range = start..end;

            // Reset the write buffer
            self.packets.viewable_self_exclusion_write_buffer.reset();
        }
        self.packets
            .viewable_self_exclusion_write_buffer
            .tick_and_maybe_shrink();

        Ok(())
    }

    fn update_client_synchronization(&mut self) -> anyhow::Result<()> {
        // Check teleport timer
        if self.teleport_id_timer > 0 {
            self.teleport_id_timer += 1;

            if self.teleport_id_timer >= 20 {
                bail!("player sent incorrect teleport id and failed to rectify within time limit");
            }
        }

        // Send keep alive timer
        self.keep_alive_timer = self.keep_alive_timer.wrapping_add(1);
        if self.keep_alive_timer == 0 {
            if self.current_keep_alive != 0 {
                bail!("client hasn't responded with keep alive")
            }
            self.current_keep_alive = rand::thread_rng().next_u64();

            self.packets.write_packet(&server::KeepAlive {
                id: self.current_keep_alive,
            });
        }

        // Send block change ack
        match self.ack_sequence_up_to {
            Some(sequence) => {
                self.packets.write_packet(&BlockChangedAck { sequence });
                self.ack_sequence_up_to = None;
            }
            None => (),
        }

        Ok(())
    }

    fn update_metadata(&mut self) -> anyhow::Result<()> {
        let write_size = self.metadata.get_write_size();
        Ok(if write_size > 0 {
            let chunk = self.get_world_mut().chunks
                .get_mut(self.chunk_view_position.x, self.chunk_view_position.z)
                .expect("chunk coords in bounds");

            packet_helper::write_metadata_packet(
                &mut chunk.entity_viewable_buffer,
                server::PacketId::SetEntityData as _,
                self.entity_id.as_i32(),
                &mut self.metadata,
            )?;
        })
    }

    fn update_equipment(&mut self, selected_hotbar_slot_changed: bool) {
        let mut equipment_changes = vec![];

        // Add MainHand if hotbar slot has changed
        if selected_hotbar_slot_changed {
            let slot = InventorySlot::Hotbar(self.selected_hotbar_slot as _);
            equipment_changes.push((EquipmentSlot::MainHand, slot));
        }

        // Add other equipment
        if self.inventory.is_any_changed() {
            // Update MainHand
            let slot = InventorySlot::Hotbar(self.selected_hotbar_slot as _);
            if self.inventory.has_changed(slot).unwrap() {
                // Abort item usage for main hand
                if self.interaction_state.using_hand == Some(Hand::Main) {
                    let interaction = self.interaction_state.try_abort_use(false).unwrap();
                    self.fire_interaction(interaction);
                }

                // Update equipment for MainHand, if needed
                if !selected_hotbar_slot_changed {
                    equipment_changes.push((EquipmentSlot::MainHand, slot));
                }
            }

            let mut write_equipment_changes =
                |inventory: InventorySlot, equipment: EquipmentSlot| {
                    if self.inventory.has_changed(inventory).unwrap() {
                        equipment_changes.push((equipment, inventory));
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
            let mut equipment = Vec::new();
            for (equipment_slot, inventory) in equipment_changes {
                let itemslot = self.inventory.get(inventory).unwrap();
                equipment.push((equipment_slot, itemslot.into()));
            }

            self.packets.write_self_excluded_viewable_packet(
                &SetEquipment {
                    entity_id: self.entity_id.as_i32(),
                    equipment,
                },
            );
        }
    }

    fn update_pose(&mut self) -> anyhow::Result<()> {
        let mut new_pose;

        // if passenger of another entity, pose should always be standing

        if self.is_fall_flying() {
            new_pose = Pose::FallFlying;
        } else if self.metadata.sleeping_pos.is_some() {
            new_pose = Pose::Sleeping;
        } else if self.is_swimming() {
            new_pose = Pose::Swimming;
        } else if self.is_spin_attacking() {
            new_pose = Pose::SpinAttack;
        } else if self.is_shift_key_down() && !self.abilities.is_flying {
            new_pose = Pose::Sneaking;
        } else {
            new_pose = Pose::Standing;
        }

        if !self.can_enter_pose(new_pose) {
            if self.can_enter_pose(Pose::Sneaking) {
                new_pose = Pose::Sneaking;
            } else {
                new_pose = Pose::Swimming;
            }
        }

        if self.metadata.pose != new_pose {
            self.metadata.set_pose(new_pose);
        }

        Ok(())
    }

    pub fn can_enter_pose(&self, _pose: Pose) -> bool {
        true // todo: actually check
    }

    fn update_interaction_state(&mut self) -> anyhow::Result<()> {
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
        Ok(for interaction in self.interaction_state.update() {
            self.fire_interaction(interaction);
        })
    }

    fn handle_movement(&mut self, to: Position, inform_client: bool) -> anyhow::Result<()> {
        let distance_sq = to.distance_sq(self.last_position);
        let rot_changed = to.rot.is_diff_u8(self.last_position.rot);
        let coord_changed = distance_sq > 0.0001;

        if coord_changed {
            // todo: maybe force a teleport if a counter is high enough
            // not sure if this is needed, because this code shouldn't result in posititional desync anyways

            if distance_sq < 8.0 * 8.0 {
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
                    self.packets.write_self_excluded_viewable_packet(&move_packet);

                    // Rotate head
                    let rotate_head = RotateHead {
                        entity_id: self.entity_id.as_i32(),
                        head_yaw: to.rot.yaw,
                    };
                    self.packets.write_self_excluded_viewable_packet(&rotate_head);
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
                    self.packets.write_self_excluded_viewable_packet(&move_packet);
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
                self.packets.write_self_excluded_viewable_packet(&teleport_packet);

                if rot_changed {
                    // Rotate head
                    let rotate_head = RotateHead {
                        entity_id: self.entity_id.as_i32(),
                        head_yaw: to.rot.yaw,
                    };
                    self.packets.write_self_excluded_viewable_packet(&rotate_head);
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
            self.packets.write_self_excluded_viewable_packet(&teleport_packet);

            // Rotate head
            let rotate_head = RotateHead {
                entity_id: self.entity_id.as_i32(),
                head_yaw: to.rot.yaw,
            };
            self.packets.write_self_excluded_viewable_packet(&rotate_head);
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
            self.packets.write_packet(&teleport_packet);
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
        match self.metadata.pose {
            Pose::Sleeping => 0.2,
            Pose::Swimming | Pose::FallFlying | Pose::SpinAttack => 0.4,
            Pose::Sneaking => 1.27,
            _ => 1.62,
        }
    }

    pub fn get_look_vector(&self) -> Vector<Real> {
        let pitch_rad = self.client_position.rot.pitch.to_radians();
        let yaw_rad = -self.client_position.rot.yaw.to_radians();
        let (pitch_sin, pitch_cos) = pitch_rad.sin_cos();
        let (yaw_sin, yaw_cos) = yaw_rad.sin_cos();
        Vector::new(yaw_sin * pitch_cos, -pitch_sin, yaw_cos * pitch_cos)
    }

    fn break_block(&mut self, pos: BlockPosition) {
        if let Some(old) = self
            .get_world_mut()
            .set_block_i32(pos.x as _, pos.y as _, pos.z as _, 0)
        {
            self.packets.write_self_excluded_viewable_packet(
                &LevelEvent {
                    event_type: LevelEventType::ParticlesDestroyBlock,
                    pos,
                    data: old as _,
                    global: false,
                },
            );

            // Update neighbors
            for offset in [(1, 0, 0), (0, 1, 0), (0, 0, 1), (-1, 0, 0), (0, -1, 0), (0, 0, -1)] {
                let x = pos.x + offset.0;
                let y = pos.y + offset.1;
                let z = pos.z + offset.2;
                
                let block_state_id = self.get_world().get_block_i32(x, y, z);
                if let Some(block_state_id) = block_state_id {
                    let block: &Block = block_state_id.try_into().unwrap();
                    let mut block = block.clone();
                    if block_update::update(block_state_id, &mut block, x, y, z, self.get_world_mut()) {
                        self.get_world_mut().set_block_i32(x, y, z, (&block).into());
                    }
                }
            }
        }
    }

    pub fn do_default_interaction(&mut self, interaction: Interaction) {
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
                position,
                face,
                offset,
            } => {
                let slot = InventorySlot::Hotbar(self.selected_hotbar_slot as _);
                let held_item = self
                    .inventory
                    .get(slot)
                    .expect("self.selected_hotbar_slot between 0..9");

                if let ItemSlot::Filled(itemstack) = held_item {
                    let item = itemstack.item;
                    // todo: decrease / change item

                    let world = self.get_world_mut();
                    if item.get_properties().has_corresponding_block {
                        let ctx_and_pos = world.create_placement_context(
                            position,
                            face,
                            offset,
                            self.position.rot,
                            item
                        );

                        if let Some(ctx_and_pos) = ctx_and_pos {
                            let mut ctx = ctx_and_pos.0;
                            let place_position = ctx_and_pos.1;

                            if let Some(block) = item.try_place(&mut ctx) {
                                let block_id: u16 = block.to_id();
                                world.set_block_i32(place_position.x, place_position.y, place_position.z, block_id);

                                // Update neighbors
                                for offset in [(1, 0, 0), (0, 1, 0), (0, 0, 1), (-1, 0, 0), (0, -1, 0), (0, 0, -1)] {
                                    let x = place_position.x + offset.0;
                                    let y = place_position.y + offset.1;
                                    let z = place_position.z + offset.2;
                                    
                                    let block_state_id = self.get_world().get_block_i32(x, y, z);
                                    if let Some(block_state_id) = block_state_id {
                                        let block: &Block = block_state_id.try_into().unwrap();
                                        let mut block = block.clone();
                                        if block_update::update(block_state_id, &mut block, x, y, z, self.get_world_mut()) {
                                            self.get_world_mut().set_block_i32(x, y, z, (&block).into());
                                        }
                                    }
                                }
                            }
                        }

                    } else if item == Item::WaterBucket {
                        // check if clicked block is waterloggable
                        let relative = position.relative(face);
                        world.set_block_i32(
                            relative.x,
                            relative.y as _,
                            relative.z,
                            Block::Water { level: 0 }.to_id(),
                        );
                    }
                }
            }
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
                    self.packets.write_self_excluded_viewable_packet(
                        &BlockDestruction {
                            entity_id: self.entity_id.as_i32(),
                            location: position,
                            destroy_stage,
                        }
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
                self.packets.write_self_excluded_viewable_packet(
                    &BlockDestruction {
                        entity_id: self.entity_id.as_i32(),
                        location: position,
                        destroy_stage: -1,
                    },
                );
            }

            Interaction::ContinuousUse { use_time, hand } => {
                if use_time == 1 {
                    self.set_using_item(true);
                    self.set_using_offhand(hand == Hand::Off);
                }
            }
            Interaction::FinishUse {
                use_time: _,
                hand: _,
            } => {
                self.set_using_item(false);
            }
            Interaction::AbortUse {
                use_time: _,
                hand: _,
                aborted_by_client: _,
            } => {
                self.set_using_item(false);
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
        self.packets.write_packet(&server::SystemChat {
            message: message.into().to_json(),
            overlay: false,
        })
    }

    pub fn disconnect(&mut self) {
        self.disconnected = true;
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
            yaw: self.position.rot.yaw,
            pitch: self.position.rot.pitch,
        };
        net::packet_helper::write_packet(write_buffer, &add_player_packet).unwrap();

        // todo: head rotation

        // todo: equipment

        // todo: metadata
    }

    pub(crate) fn write_packet_bytes(&mut self, bytes: &[u8]) {
        self.packets.write_buffer.copy_from(bytes);
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
            let to_write = self.packets.write_buffer.get_written();
            if !to_write.is_empty() {
                self.connection.write_bytes(to_write);
            }
            self.packets.write_buffer.reset();
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
            let chunks = &mut self.get_world_mut().chunks;
            if let Some(old_chunk) = chunks.get_mut(self.chunk_view_position.x, self.chunk_view_position.z) {
                old_chunk.destroy_player(self);
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
        // Safety: player pointer is valid, constructed above
        unsafe { self.connection.update_player_pointer(ptr); }
        
        let chunk = self.get_world_mut().chunks
            .get_mut(self.chunk_view_position.x, self.chunk_view_position.z)
            .expect("chunk coords in bounds");
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
