use std::{rc::Rc, cell::RefCell, ptr::NonNull};

use anyhow::bail;
use downcast_rs::Downcast;
use graphite_binary::slice_serialization::SliceSerializable;
use graphite_mc_protocol::{IdentifiedPacket, play::{serverbound::{PacketHandler, AcceptTeleportation, KeepAlive, MovePlayerPosRot, MovePlayerPos, PlayerHandAction, MovePlayerRot, UseItemOn, SetCreativeModeSlot, SetCarriedItem}, self, clientbound::{RemoveEntities, BlockChangedAck}}};
use graphite_network::{PacketBuffer, PacketWriteError, Connection, FramedPacketHandler, HandleAction};
use glam::{DVec3, Vec3};
use rand::RngCore;

use crate::{world::{WorldExtension, World, chunk_view_diff::{self, ChunkDiffStatus}}, inventory::{Inventory, item_stack::ItemStack, InventorySlot}, Behaviour};

pub trait GenericPlayer: Downcast {
    fn is_valid(&self) -> bool;
    fn tick(&mut self);
    fn view_tick(&mut self);
}
downcast_rs::impl_downcast!(GenericPlayer);

impl <P: PlayerExtension + 'static> GenericPlayer for Player<P> {
    fn is_valid(&self) -> bool {
        self.connection.is_some()
    }

    fn tick(&mut self) {
        <Player<P>>::tick(self);
    }

    fn view_tick(&mut self) {
        <Player<P>>::view_tick(self);
    }
}

pub trait PlayerExtension: Sized + 'static {
    type World: WorldExtension;
    const DEFAULT_BEHAVIOUR: Behaviour = Behaviour::Pass;

    fn swap_item_with_off_hand(_player: &mut Player<Self>) -> Behaviour {
        Self::DEFAULT_BEHAVIOUR
    }

    fn set_creative_mode_slot(_player: &mut Player<Self>, _slot: i16, _item_stack: Option<&ItemStack>) -> Behaviour {
        Self::DEFAULT_BEHAVIOUR
    }
}

pub struct Player<P: PlayerExtension> {
    world: NonNull<World<P::World>>,
    pub(crate) connection: Option<Rc<RefCell<Connection>>>,
    pub(crate) packet_buffer: PacketBuffer,

    // General fields
    last_position: DVec3,
    position: DVec3,
    yaw: f32,
    pitch: f32,

    inventory: Inventory,
    hotbar_slot: u8,

    // Fields for packet handling
    pending_teleport: Option<PendingTeleport>,
    ack_sequence_up_to: Option<i32>,
    keep_alive_timer: u8,
    current_keep_alive: u64,

    // Extension
    extension: P
}

impl <P: PlayerExtension> Player<P> {
    pub fn new(world: &mut World<P::World>, position: DVec3, connection: Rc<RefCell<Connection>>, extension: P) -> Self {
        Self {
            world: world.into(),
            connection: Some(connection),
            packet_buffer: PacketBuffer::new(),

            last_position: position,
            position,
            yaw: 0.0,
            pitch: 0.0,

            inventory: Inventory::new(),
            hotbar_slot: 0,

            pending_teleport: None,
            ack_sequence_up_to: None,
            keep_alive_timer: 0,
            current_keep_alive: 0,

            extension
        }
    }

    pub fn position(&self) -> DVec3 {
        self.position
    }

    pub fn yaw(&self) -> f32 {
        self.yaw
    }

    pub fn pitch(&self) -> f32 {
        self.pitch
    }

    pub fn view_vector(&self) -> Vec3 {
        let (yaw_sin, yaw_cos) = self.yaw.to_radians().sin_cos();
        let (pitch_sin, pitch_cos) = self.pitch.to_radians().sin_cos();

        Vec3::new(
            -yaw_sin * pitch_cos,
            -pitch_sin,
            yaw_cos * pitch_cos
        )
    }

    pub fn set_velocity(&mut self, velocity: Vec3) {
        self.packet_buffer.write_packet(&play::clientbound::SetEntityMotion {
            entity_id: 0,
            x_vel: velocity.x,
            y_vel: velocity.y,
            z_vel: velocity.z,
        }).unwrap();
    }

    pub fn teleport_position(&mut self, position: DVec3) {
        self.teleport_full(0.0, 0.0, position, 0b11000)
    }

    pub fn teleport_full(&mut self, yaw: f32, pitch: f32, position: DVec3, relative_arguments: u8) {
        let teleport_id = rand::random();

        self.pending_teleport = Some(PendingTeleport {
            awaiting_position: position,
            pending_teleport_id: teleport_id,
            pending_teleport_time: 20,
        });

        self.position = position;

        self.packet_buffer.write_packet(&play::clientbound::PlayerPosition {
            x: position.x,
            y: position.y,
            z: position.z,
            yaw,
            pitch,
            relative_arguments,
            id: teleport_id,
        }).unwrap();
    }
    
    pub fn extension(&mut self) -> &mut P {
        &mut self.extension
    }

    pub fn world(&self) -> &World<P::World> {
        unsafe {
            self.world.as_ref()
        }
    }

    pub fn world_mut(&mut self) -> &mut World<P::World> {
        unsafe {
            self.world.as_mut()
        }
    }

    pub fn write_packet<'a, I: std::fmt::Debug, T>(&mut self, packet: &'a T) -> Result<(), PacketWriteError>
    where
        T: SliceSerializable<'a, T> + IdentifiedPacket<I> + 'a,
    {
        self.packet_buffer.write_packet(packet)
    }

    pub fn flush_packets(&mut self) {
        if let Some(connection) = &self.connection {
            connection.borrow_mut().send(self.packet_buffer.pop_written())
        } else {
            self.packet_buffer.clear();
        }
    }

    fn tick(&mut self) {
        // Send keep alive timer
        self.keep_alive_timer = self.keep_alive_timer.wrapping_add(1);
        if self.keep_alive_timer == 0 {
            if self.current_keep_alive != 0 {
                // todo: kick player
            }
            self.current_keep_alive = rand::thread_rng().next_u64();

            self.write_packet(&play::clientbound::KeepAlive {
                id: self.current_keep_alive,
            }).unwrap();
        }

        // Synchronize inventory
        self.inventory.synchronize(&mut self.packet_buffer);

        if let Some(pending_teleport) = &mut self.pending_teleport {
            pending_teleport.pending_teleport_time -= 1;
            if pending_teleport.pending_teleport_time <= 0 {
                // self.teleport(yaw, pitch, position);
                pending_teleport.pending_teleport_time = 20;
            }
        }

        // Update position
        let chunk_x = (self.position.x / 16.0).floor() as i32;
        let chunk_z = (self.position.z / 16.0).floor() as i32;
        let last_chunk_x = (self.last_position.x / 16.0).floor() as i32;
        let last_chunk_z = (self.last_position.z / 16.0).floor() as i32;

        if chunk_x != last_chunk_x || chunk_z != last_chunk_z {
            let world = unsafe { self.world.as_mut() };

            self.write_packet(&play::clientbound::SetChunkCacheCenter {
                chunk_x,
                chunk_z,
            }).unwrap();

            let mut despawn_list = vec![];

            let delta = (chunk_x - last_chunk_x, chunk_z - last_chunk_z);

            // Chunks use VIEW_DISTANCE + 1
            chunk_view_diff::for_each_diff(delta, P::World::VIEW_DISTANCE + 1, 
                |dx, dz, status| {
                    if status == ChunkDiffStatus::New {
                        if let Some(chunk) = world.get_chunk_mut(last_chunk_x+dx, last_chunk_z+dz) {
                            chunk.write(&mut self.packet_buffer, last_chunk_x+dx, last_chunk_z+dz);
                        } else {
                            world.empty_chunk.write(&mut self.packet_buffer, last_chunk_x+dx, last_chunk_z+dz);
                        }
                    }
                }
            );

            // Entities use VIEW_DISTANCE
            chunk_view_diff::for_each_diff(delta, P::World::VIEW_DISTANCE, 
                |dx, dz, status| {
                    if status == ChunkDiffStatus::New {
                        if let Some(chunk) = world.get_chunk_mut(last_chunk_x+dx, last_chunk_z+dz) {
                            // Spawn entities
                            chunk.write_spawn_entities_and_players(&mut self.packet_buffer);
                        }
                    } else if let Some(chunk) = world.get_chunk_mut(last_chunk_x+dx, last_chunk_z+dz) {
                        chunk.write_despawn_entities_and_players(&mut despawn_list, &mut self.packet_buffer);
                    }
                }
            );

            if !despawn_list.is_empty() {
                let remove_entities = RemoveEntities {
                    entities: despawn_list,
                };
                let _ = self.write_packet(&remove_entities);
            }
        }

        self.last_position = self.position;
        self.flush_packets();
    }

    fn view_tick(&mut self) {
        let world = unsafe {
            self.world.as_mut()
        };

        let chunk_x = (self.position.x.floor() as i32) >> 4;
        let chunk_z = (self.position.z.floor() as i32) >> 4;

        // Chunk viewable
        let view_distance = P::World::VIEW_DISTANCE as i32 + 1;
        for x in (chunk_x-view_distance).max(0) .. (chunk_x+view_distance+1).min(P::World::CHUNKS_X) {
            for z in (chunk_z-view_distance).max(0) .. (chunk_z+view_distance+1).min(P::World::CHUNKS_Z) {
                let chunk = world.get_chunk_mut(x, z).unwrap();
                chunk.copy_chunk_viewable_packets(&mut self.packet_buffer);
            }
        }

        // Entity viewable
        let view_distance = P::World::VIEW_DISTANCE as i32;
        for x in (chunk_x-view_distance).max(0) .. (chunk_x+view_distance+1).min(P::World::CHUNKS_X) {
            for z in (chunk_z-view_distance).max(0) .. (chunk_z+view_distance+1).min(P::World::CHUNKS_Z) {
                let chunk = world.get_chunk_mut(x, z).unwrap();
                chunk.copy_entity_viewable_packets(&mut self.packet_buffer);
            }
        }

        // Send block change ack
        if let Some(ack_sequence_up_to) = self.ack_sequence_up_to {
            self.write_packet(&BlockChangedAck { sequence: ack_sequence_up_to }).unwrap();
            self.ack_sequence_up_to = None;
        }
    }
}

pub struct PendingTeleport {
    awaiting_position: DVec3,
    pending_teleport_id: i32,
    pending_teleport_time: i32,
}

impl <P: PlayerExtension> FramedPacketHandler for Player<P> {
    fn handle(&mut self, data: &[u8]) -> HandleAction {
        match self.parse_and_handle(data) {
            Ok(()) => HandleAction::Continue,
            Err(error) => if cfg!(debug_assertions) {
                panic!("Encountered error handling packet: {}", error);
            } else {
                HandleAction::Disconnect
            },
        }
    }

    fn disconnected(&mut self) {
        self.connection = None;
    }
}

impl <P: PlayerExtension> graphite_mc_protocol::play::serverbound::PacketHandler for Player<P> {
    const DEBUG: bool = false;

    fn handle_accept_teleportation(&mut self, accept_teleportation: AcceptTeleportation) -> anyhow::Result<()> {
        if let Some(pending_teleport) = &mut self.pending_teleport {
            if pending_teleport.pending_teleport_id == accept_teleportation.id {
                self.position = pending_teleport.awaiting_position;
                self.pending_teleport = None;
            }
        }

        Ok(())
    }

    fn handle_player_hand_action(&mut self, player_hand_action: PlayerHandAction) -> anyhow::Result<()> {
        self.ack_sequence_up_to = Some(player_hand_action.sequence);

        match player_hand_action.action {
            graphite_mc_protocol::types::HandAction::StartDestroyBlock => {
                let pos = player_hand_action.block_pos;
                let chunk = self.world_mut().get_chunk_mut(pos.x >> 4, 
                    pos.z >> 4).unwrap();
                chunk.set_block(pos.x, pos.y, pos.z, 0);
                Ok(())
            },
            graphite_mc_protocol::types::HandAction::AbortDestroyBlock => Ok(()),
            graphite_mc_protocol::types::HandAction::StopDestroyBlock => Ok(()),
            graphite_mc_protocol::types::HandAction::DropAllItems => Ok(()),
            graphite_mc_protocol::types::HandAction::DropItem => Ok(()),
            graphite_mc_protocol::types::HandAction::ReleaseUseItem => Ok(()),
            graphite_mc_protocol::types::HandAction::SwapItemWithOffHand => {
                P::swap_item_with_off_hand(self);
                Ok(())
            },
        }
    }

    fn handle_use_item_on(&mut self, use_item_on: UseItemOn) -> anyhow::Result<()> {
        self.ack_sequence_up_to = Some(use_item_on.sequence);

        if let Some(item_stack) = self.inventory.get(InventorySlot::Hotbar(self.hotbar_slot as usize)) {
            if let Some(block) = item_stack.properties().corresponding_block {
                let pos = use_item_on.block_hit.position;
                let pos = pos.relative(use_item_on.block_hit.direction);
                let chunk = self.world_mut().get_chunk_mut(pos.x >> 4, 
                    pos.z >> 4).unwrap();
                chunk.set_block(pos.x, pos.y, pos.z, block);
            }
        }

        Ok(())   
    }

    fn handle_set_carried_item(&mut self, set_carried_item: SetCarriedItem) -> anyhow::Result<()> {
        if set_carried_item.slot > 8 {
            bail!("invalid slot")
        }

        self.hotbar_slot = set_carried_item.slot as u8;

        Ok(())
    }

    fn handle_keep_alive(&mut self, keep_alive: KeepAlive) -> anyhow::Result<()> {
        if self.current_keep_alive == keep_alive.id {
            self.current_keep_alive = 0;
        }
        Ok(())
    }

    fn handle_move_player_pos(&mut self, move_player_pos: MovePlayerPos) -> anyhow::Result<()> {
        if self.pending_teleport.is_some() {
            return Ok(());
        }

        self.position.x = move_player_pos.x;
        self.position.y = move_player_pos.y;
        self.position.z = move_player_pos.z;
        Ok(())
    }

    fn handle_set_creative_mode_slot(&mut self, set_creative_mode_slot: SetCreativeModeSlot) -> anyhow::Result<()> {
        let Ok(inventory_slot) = InventorySlot::from_index(set_creative_mode_slot.slot) else {
            bail!("invalid slot: {}", set_creative_mode_slot.slot);
        };

        let item_stack = match set_creative_mode_slot.item {
            Some(item_stack) => Some(item_stack.try_into()?),
            None => None,
        };

        if P::set_creative_mode_slot(self, set_creative_mode_slot.slot, item_stack.as_ref()) == Behaviour::Vanilla {
            self.inventory.set(inventory_slot, item_stack.clone())
        }

        self.inventory.mark_modified_by_client(set_creative_mode_slot.slot as usize, item_stack);

        Ok(())
    }

    fn handle_move_player_pos_rot(&mut self, move_player_pos_rot: MovePlayerPosRot) -> anyhow::Result<()> {
        if self.pending_teleport.is_some() {
            return Ok(());
        }

        self.position.x = move_player_pos_rot.x;
        self.position.y = move_player_pos_rot.y;
        self.position.z = move_player_pos_rot.z;
        self.yaw = move_player_pos_rot.yaw;
        self.pitch = move_player_pos_rot.pitch;
        Ok(())
    }

    fn handle_move_player_rot(&mut self, move_player_rot: MovePlayerRot) -> anyhow::Result<()> {
        if self.pending_teleport.is_some() {
            return Ok(());
        }

        self.yaw = move_player_rot.yaw;
        self.pitch = move_player_rot.pitch;
        Ok(())
    }
}