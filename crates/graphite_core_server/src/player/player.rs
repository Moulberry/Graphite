use std::{borrow::Cow, cell::{Cell, RefCell}, ptr::NonNull, rc::Rc};

use anyhow::bail;
use downcast_rs::Downcast;
use graphite_binary::{slice_serialization::SliceSerializable, nbt::CachedNBT};
use graphite_mc_constants::{entity::{Entity, Metadata, PlayerMetadata}, item::Item};
use graphite_mc_protocol::{play::{self, clientbound::{BlockChangedAck, RemoveEntities}, serverbound::{AcceptTeleportation, KeepAlive, MovePlayerPos, MovePlayerPosRot, MovePlayerRot, PacketHandler, PlayerHandAction, SetCarriedItem, SetCreativeModeSlot, UseItemOn}}, types::{Hand, Position}, IdentifiedPacket};
use graphite_network::{PacketBuffer, PacketWriteError, Connection, FramedPacketHandler, HandleAction};
use glam::{DVec3, Vec2, Vec3};
use rand::RngCore;

use crate::{entity::next_entity_id, inventory::{item_stack::ItemStack, Inventory, InventorySlot, ItemHolder}, text::TextComponent, types::AABB, world::{chunk::ChunkPlayerRef, chunk_view_diff::{self, ChunkDiffStatus}, EntityId, PlayerId, World, WorldExtension}, Behaviour};

pub trait GenericPlayer: Downcast {
    fn is_valid(&self) -> bool;
    fn tick(&mut self);
    fn view_tick(&mut self);
    fn send_packet_data(&mut self, data: &[u8]);
}
downcast_rs::impl_downcast!(GenericPlayer);

impl <P: PlayerExtension + 'static> GenericPlayer for Player<P> {
    fn is_valid(&self) -> bool {
        self.connection.is_some() && self.self_id.is_some()
    }

    fn tick(&mut self) {
        <Player<P>>::tick(self);
    }

    fn view_tick(&mut self) {
        <Player<P>>::view_tick(self);
    }

    fn send_packet_data(&mut self, data: &[u8]) {
        self.packet_buffer.copy_bytes(data);
    }
}

pub enum TickUsingItemResult {
    Continue,
    Finish,
    Abort
}

pub trait PlayerExtension: Sized + 'static {
    type World: WorldExtension;
    type InventoryItemHolder: ItemHolder;
    const DEFAULT_BEHAVIOUR: Behaviour = Behaviour::DoNothing;

    fn tick(_player: &mut Player<Self>) {
    }

    fn interact_entity_at(player: &mut Player<Self>, entity_id: EntityId, hand: Hand, pos: Vec3) -> bool {
        false
    }

    fn tick_using_item(_player: &mut Player<Self>, slot: InventorySlot, ticks: usize) -> TickUsingItemResult {
        TickUsingItemResult::Abort
    }

    fn use_item(_player: &mut Player<Self>, hand: Hand) -> bool {
        false
    }

    fn finish_using_item(_player: &mut Player<Self>, slot: InventorySlot, ticks: usize) {
    }

    fn abort_using_item(_player: &mut Player<Self>) {
    }

    fn attack_strength_reset(_player: &mut Player<Self>) {
    }

    fn swap_item_with_off_hand(_player: &mut Player<Self>) -> Behaviour {
        Self::DEFAULT_BEHAVIOUR
    }

    fn set_creative_mode_slot(_player: &mut Player<Self>, _slot: i16, _item_stack: Option<&ItemStack>) {
    }

    fn attack_entity(_player: &mut Player<Self>, _entity: EntityId) {
    }
}

pub struct Player<P: PlayerExtension> {
    world: NonNull<World<P::World>>,
    pub(crate) connection: Option<Rc<RefCell<Connection>>>,
    pub(crate) packet_buffer: PacketBuffer,
    pub self_id: Option<PlayerId>,
    pub entity_id: i32, 

    // General fields
    last_position: DVec3,
    pub position: DVec3,
    pub on_ground: bool,
    pub yaw: f32,
    pub pitch: f32,
    pub(crate) chunk_ref: Option<ChunkPlayerRef>,

    pub inventory: Inventory<P::InventoryItemHolder>,
    pub hotbar_slot: u8,
    pub metadata: PlayerMetadata,
    pub attack_strength_ticker: usize,
    held_item_type: Item,

    // Fields for packet handling
    pending_teleport: Option<PendingTeleport>,
    ack_sequence_up_to: Option<i32>,
    keep_alive_timer: u8,
    current_keep_alive: u64,
    using_item: Option<UsingItem>,
    interacted_with_entity: bool,

    // Extension
    pub extension: P
}

impl <P: PlayerExtension> Player<P> {
    pub fn new(world: &mut World<P::World>, position: DVec3, connection: Rc<RefCell<Connection>>, extension: P) -> Self {
        Self {
            world: world.into(),
            connection: Some(connection),
            packet_buffer: PacketBuffer::new(),
            self_id: None,
            entity_id: next_entity_id(),

            last_position: position,
            position,
            on_ground: false,
            yaw: 0.0,
            pitch: 0.0,
            chunk_ref: None,

            inventory: Inventory::new(),
            hotbar_slot: 0,
            metadata: PlayerMetadata::default(),
            attack_strength_ticker: 0,
            held_item_type: Item::Air,

            pending_teleport: None,
            ack_sequence_up_to: None,
            keep_alive_timer: 0,
            current_keep_alive: 0,
            using_item: None,
            interacted_with_entity: false,

            extension
        }
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

    pub fn view_vector_xz(&self) -> Vec2 {
        let (yaw_sin, yaw_cos) = self.yaw.to_radians().sin_cos();

        Vec2::new(
            -yaw_sin,
            yaw_cos
        )
    }

    pub fn send_packet<'a, I: std::fmt::Debug, T>(&mut self, packet: &'a T)
    where
        T: SliceSerializable<'a, T> + IdentifiedPacket<I> + 'a,
    {
        self.packet_buffer.write_packet(packet).unwrap();
    }

    pub fn send_velocity(&mut self, velocity: Vec3) {
        self.packet_buffer.write_packet(&play::clientbound::SetEntityMotion {
            entity_id: self.entity_id,
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

    pub fn send_damage_without_source_entity(&mut self, source_type_id: i32, source_position: Option<DVec3>) {
        self.packet_buffer.write_packet(&play::clientbound::DamageEvent {
            entity_id: self.entity_id,
            source_type_id,
            source_cause_id: 0,
            source_direct_id: 0,
            source_position: source_position.map(|v| Position { x: v.x, y: v.y, z: v.z })
        }).unwrap(); 
    }

    pub fn send_chat_message(&mut self, message: &str) {
        self.packet_buffer.write_packet(&play::clientbound::SystemChat {
            message: Cow::Owned(TextComponent {
                text: message,
                font: None,
                color: None
            }.to_nbt().into()),
            overlay: false,
        }).unwrap();
    }

    pub fn send_action_bar_message_str(&mut self, message: &str) {
        self.packet_buffer.write_packet(&play::clientbound::SystemChat {
            message: Cow::Owned(TextComponent {
                text: message,
                font: None,
                color: None
            }.to_nbt().into()),
            overlay: true,
        }).unwrap();
    }

    pub fn send_action_bar_message(&mut self, message: TextComponent) {
        self.packet_buffer.write_packet(&play::clientbound::SystemChat {
            message: Cow::Owned(message.to_nbt().into()),
            overlay: true,
        }).unwrap();
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

    pub fn create_collision_aabb(&self) -> AABB {
        const WIDTH: f64 = 0.6;
        const HEIGHT: f64 = 1.8;
        const HALF_WIDTH: f64 = WIDTH * 0.5;

        let min = DVec3::new(self.position.x - HALF_WIDTH, self.position.y, self.position.z - HALF_WIDTH);
        let max = DVec3::new(self.position.x + HALF_WIDTH, self.position.y + HEIGHT, self.position.z + HALF_WIDTH);
        AABB::new(min, max).unwrap()
    }

    pub fn create_crawling_collision_aabb(&self) -> AABB {
        const WIDTH: f64 = 0.6;
        const HEIGHT: f64 = 0.6;
        const HALF_WIDTH: f64 = WIDTH * 0.5;

        let min = DVec3::new(self.position.x - HALF_WIDTH, self.position.y, self.position.z - HALF_WIDTH);
        let max = DVec3::new(self.position.x + HALF_WIDTH, self.position.y + HEIGHT, self.position.z + HALF_WIDTH);
        AABB::new(min, max).unwrap()
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
        P::tick(self);

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

        self.interacted_with_entity = false;

        let hotbar_slot = InventorySlot::Hotbar(self.hotbar_slot as usize);
        let held_item = self.inventory.get_item_stack(hotbar_slot).item;

        if held_item != self.held_item_type {
            self.held_item_type = held_item;
            self.attack_strength_ticker = 0;
            P::attack_strength_reset(self);
        } else {
            self.attack_strength_ticker = self.attack_strength_ticker.saturating_add(1);
        }

        if let Some(using_item) = &mut self.using_item {
            using_item.ticks += 1;

            if using_item.slot == hotbar_slot && using_item.item_stack.item == held_item {
                let ticks = using_item.ticks;
                let slot = using_item.slot;
                match P::tick_using_item(self, slot, ticks) {
                    TickUsingItemResult::Continue => {},
                    TickUsingItemResult::Finish => {
                        self.metadata.set_living_entity_flags(self.metadata.living_entity_flags & !1);
                        P::finish_using_item(self, slot, ticks);
                        self.using_item = None;
                    },
                    TickUsingItemResult::Abort => {
                        self.try_abort_using_item();
                    },
                }
            } else {
                self.try_abort_using_item();
            }
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

        // Update metadata
        self.metadata.write_metadata_changes_packet(self.entity_id, &mut self.packet_buffer).unwrap();

        // Update position
        let chunk_x = (self.position.x / 16.0).floor() as i32;
        let chunk_z = (self.position.z / 16.0).floor() as i32;
        let last_chunk_x = (self.last_position.x / 16.0).floor() as i32;
        let last_chunk_z = (self.last_position.z / 16.0).floor() as i32;

        if chunk_x != last_chunk_x || chunk_z != last_chunk_z {
            if let Some(chunk_ref) = self.chunk_ref.take() {
                let chunk = self.world_mut().get_chunk_mut(last_chunk_x, last_chunk_z).unwrap();
                chunk.remove_player(chunk_ref);
            }

            let id = self.self_id.clone().unwrap();
            self.chunk_ref = self.world_mut().put_player_into_chunk(id, chunk_x, chunk_z);

            let world = unsafe { self.world.as_mut() };

            self.write_packet(&play::clientbound::SetChunkCacheCenter {
                chunk_x,
                chunk_z,
            }).unwrap();

            // todo: use a thread local?
            let mut despawn_list = vec![];

            let delta = (chunk_x - last_chunk_x, chunk_z - last_chunk_z);

            // Chunks use VIEW_DISTANCE
            chunk_view_diff::for_each_diff(delta, P::World::VIEW_DISTANCE, 
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

            // Entities use ENTITY_VIEW_DISTANCE
            chunk_view_diff::for_each_diff(delta, P::World::ENTITY_VIEW_DISTANCE, 
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
                    entities: despawn_list.into(),
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
        let view_distance = P::World::VIEW_DISTANCE as i32;
        for x in (chunk_x-view_distance).max(0) .. (chunk_x+view_distance+1).min(P::World::CHUNKS_X) {
            for z in (chunk_z-view_distance).max(0) .. (chunk_z+view_distance+1).min(P::World::CHUNKS_Z) {
                let chunk = world.get_chunk_mut(x, z).unwrap();
                chunk.copy_chunk_viewable_packets(&mut self.packet_buffer);
            }
        }

        // Entity viewable
        let view_distance = P::World::ENTITY_VIEW_DISTANCE as i32;
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

    fn try_abort_using_item(&mut self) {
        if self.using_item.is_some() {
            P::abort_using_item(self);
            self.metadata.set_living_entity_flags(self.metadata.living_entity_flags & !1);
            self.using_item = None;
        }
    }

    fn try_begin_using_item(&mut self, hand: Hand) -> bool {
        let hotbar_slot = InventorySlot::Hotbar(self.hotbar_slot as usize);
        let item_stack = self.inventory.get_item_stack(hotbar_slot);

        if !item_stack.is_empty() {
            let item_stack = item_stack.clone();

            if let Some(using_item) = &self.using_item {
                if using_item.slot == hotbar_slot && using_item.item_stack.item == item_stack.item {
                   return true; 
                } else {
                    self.try_abort_using_item();
                }
            }

            if P::use_item(self, hand) {
                self.metadata.set_living_entity_flags(self.metadata.living_entity_flags | 1);
                self.using_item = Some(UsingItem {
                    item_stack,
                    slot: hotbar_slot,
                    ticks: 0
                });
                return true;
            }
        }

        return false;
    }
}

struct PendingTeleport {
    awaiting_position: DVec3,
    pending_teleport_id: i32,
    pending_teleport_time: i32,
}

struct UsingItem {
    item_stack: ItemStack,
    slot: InventorySlot,
    ticks: usize
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
                self.try_abort_using_item();
                Ok(())
            },
            graphite_mc_protocol::types::HandAction::AbortDestroyBlock => {
                self.try_abort_using_item();
                Ok(())
            },
            graphite_mc_protocol::types::HandAction::StopDestroyBlock => {
                self.try_abort_using_item();
                Ok(())
            },
            graphite_mc_protocol::types::HandAction::DropAllItems => {
                let hotbar_slot = InventorySlot::Hotbar(self.hotbar_slot as usize);
                self.inventory.mark_modified_by_client(hotbar_slot.get_index().unwrap(), Some(ItemStack::EMPTY));
                Ok(())
            },
            graphite_mc_protocol::types::HandAction::DropItem => {
                let hotbar_slot = InventorySlot::Hotbar(self.hotbar_slot as usize);
                self.inventory.mark_modified_by_client(hotbar_slot.get_index().unwrap(), None);
                Ok(())
            },
            graphite_mc_protocol::types::HandAction::ReleaseUseItem => {
                let hotbar_slot = InventorySlot::Hotbar(self.hotbar_slot as usize);

                if let Some(using_item) = &self.using_item {
                    if using_item.slot == hotbar_slot &&
                            using_item.item_stack.item == self.inventory.get_item_stack(hotbar_slot).item {
                        P::finish_using_item(self, using_item.slot, using_item.ticks);
                    }
                }
                
                self.using_item = None;
                self.inventory.mark_modified_by_client(hotbar_slot.get_index().unwrap(), None);
                Ok(())
            },
            graphite_mc_protocol::types::HandAction::SwapItemWithOffHand => {
                P::swap_item_with_off_hand(self);
                Ok(())
            },
        }
    }

    fn handle_use_item_on(&mut self, use_item_on: UseItemOn) -> anyhow::Result<()> {
        self.ack_sequence_up_to = Some(use_item_on.sequence);
        let hotbar_slot = InventorySlot::Hotbar(self.hotbar_slot as usize);
        self.inventory.mark_modified_by_client(hotbar_slot.get_index().unwrap(), None);

        if self.interacted_with_entity {
            return Ok(());
        }

        if self.try_begin_using_item(use_item_on.hand) {
            return Ok(());
        }
        self.try_abort_using_item();

        let hotbar_slot = InventorySlot::Hotbar(self.hotbar_slot as usize);
        let item_stack = self.inventory.get_item_stack(hotbar_slot);

        if let Some(block) = item_stack.properties().corresponding_block {
            if block == 0 {
                return Ok(());
            }

            let pos = use_item_on.block_hit.position;
            let pos = pos.relative(use_item_on.block_hit.direction);
            let chunk = self.world_mut().get_chunk_mut(pos.x >> 4, 
                pos.z >> 4).unwrap();
            chunk.set_block(pos.x, pos.y, pos.z, block);
        }

        Ok(())   
    }

    fn handle_use_item(&mut self, use_item: play::serverbound::UseItem) -> anyhow::Result<()> {
        self.ack_sequence_up_to = Some(use_item.sequence);
        let hotbar_slot = InventorySlot::Hotbar(self.hotbar_slot as usize);
        self.inventory.mark_modified_by_client(hotbar_slot.get_index().unwrap(), None);

        if self.interacted_with_entity {
            return Ok(());
        }

        if self.try_begin_using_item(use_item.hand) {
            return Ok(());
        }
        self.try_abort_using_item();

        Ok(())
    }

    fn handle_swing(&mut self, swing: play::serverbound::Swing) -> anyhow::Result<()> {
        if self.attack_strength_ticker > 0 {
            self.attack_strength_ticker = 0;
            P::attack_strength_reset(self);
        }
        Ok(())
    }

    fn handle_interact_entity(&mut self, interact_entity: play::serverbound::InteractEntity) -> anyhow::Result<()> {
        if self.entity_id == interact_entity.entity_id {
            bail!("invalid attack target")
        }

        self.try_abort_using_item();

        if let Some(entity) = self.world_mut().entities_by_network_id.get(&interact_entity.entity_id) {
            let entity = unsafe { entity.get().as_ref() }.unwrap();
            if let Some(entity_id) = entity.get_self_id() {
                match interact_entity.mode {
                    play::serverbound::InteractMode::Interact { hand } => {
                        if self.interacted_with_entity {
                            return Ok(());
                        }

                        if self.try_begin_using_item(hand) {
                            return Ok(());
                        }
                        self.try_abort_using_item();
                    },
                    play::serverbound::InteractMode::Attack {  } => {
                        P::attack_entity(self, entity_id);
                        self.attack_strength_ticker = 0;
                        P::attack_strength_reset(self);
                    },
                    play::serverbound::InteractMode::InteractAt { offset_x, offset_y, offset_z, hand } => {
                        if self.interacted_with_entity {
                            return Ok(());
                        }

                        if P::interact_entity_at(self, entity_id, hand, Vec3::new(offset_x, offset_y, offset_z)) {
                            self.try_abort_using_item();
                            self.interacted_with_entity = true;
                            return Ok(());
                        }

                        if self.try_begin_using_item(hand) {
                            return Ok(());
                        }
                        self.try_abort_using_item();
                    },
                }
            }
        }

        Ok(())
    }

    fn handle_set_carried_item(&mut self, set_carried_item: SetCarriedItem) -> anyhow::Result<()> {
        if set_carried_item.slot > 8 {
            bail!("invalid slot")
        }

        if self.hotbar_slot != set_carried_item.slot as u8 {
            self.try_abort_using_item();

            let hotbar_slot = InventorySlot::Hotbar(set_carried_item.slot as usize);
            let held_item = self.inventory.get_item_stack(hotbar_slot).item;
            if held_item != self.held_item_type {
                self.held_item_type = held_item;
                self.attack_strength_ticker = 0;
                P::attack_strength_reset(self);
            }

            self.hotbar_slot = set_carried_item.slot as u8;
        }

        Ok(())
    }

    fn handle_keep_alive(&mut self, keep_alive: KeepAlive) -> anyhow::Result<()> {
        if self.current_keep_alive == keep_alive.id {
            self.current_keep_alive = 0;
        }
        Ok(())
    }

    fn handle_set_creative_mode_slot(&mut self, set_creative_mode_slot: SetCreativeModeSlot) -> anyhow::Result<()> {
        let Ok(_) = InventorySlot::from_index(set_creative_mode_slot.slot) else {
            bail!("invalid slot: {}", set_creative_mode_slot.slot);
        };

        // todo: check if in creative!

        self.try_abort_using_item();

        let item_stack: ItemStack = set_creative_mode_slot.item.try_into()?;
        let item_stack = item_stack.not_empty();

        P::set_creative_mode_slot(self, set_creative_mode_slot.slot, item_stack.as_ref());

        self.inventory.mark_modified_by_client(set_creative_mode_slot.slot as usize, item_stack);

        Ok(())
    }

    fn handle_move_player_pos(&mut self, move_player: MovePlayerPos) -> anyhow::Result<()> {
        if self.pending_teleport.is_some() {
            return Ok(());
        }

        if is_invalid_movement(move_player.x, move_player.y, move_player.z, 0.0, 0.0) {
            bail!("invalid move value");
        }

        let aabb = self.create_crawling_collision_aabb();
        let move_delta = DVec3::new(move_player.x - self.position.x, move_player.y - self.position.y,
            move_player.z - self.position.z);

        if move_delta.length_squared() > 100.0 {
            self.teleport_position(self.position);
            return Ok(());
        }

        let (delta, hit_x, hit_y, hit_z) = self.world_mut().move_bounding_box_with_collision(aabb, move_delta);

        self.position.x += delta.x;
        self.position.y += delta.y;
        self.position.z += delta.z;

        if (hit_x || hit_y || hit_z) && move_delta.distance_squared(delta) > 0.5 {
            self.teleport_position(self.position);
        }

        self.on_ground = move_player.on_ground;
        Ok(())
    }

    fn handle_move_player_on_ground(&mut self, move_player_on_ground: play::serverbound::MovePlayerOnGround) -> anyhow::Result<()> {
        if self.pending_teleport.is_some() {
            return Ok(());
        }

        self.on_ground = move_player_on_ground.on_ground;
        Ok(())
    }

    fn handle_move_player_pos_rot(&mut self, move_player: MovePlayerPosRot) -> anyhow::Result<()> {
        if self.pending_teleport.is_some() {
            return Ok(());
        }

        if is_invalid_movement(move_player.x, move_player.y, move_player.z, move_player.yaw, move_player.pitch) {
            bail!("invalid move value");
        }

        let aabb = self.create_crawling_collision_aabb();
        let move_delta = DVec3::new(move_player.x - self.position.x, move_player.y - self.position.y,
            move_player.z - self.position.z);

        if move_delta.length_squared() > 100.0 {
            self.teleport_position(self.position);
            return Ok(());
        }

        let (delta, hit_x, hit_y, hit_z) = self.world_mut().move_bounding_box_with_collision(aabb, move_delta);

        self.position.x += delta.x;
        self.position.y += delta.y;
        self.position.z += delta.z;

        if (hit_x || hit_y || hit_z) && move_delta.distance_squared(delta) > 0.5 {
            self.teleport_position(self.position);
        }

        self.yaw = move_player.yaw;
        self.pitch = move_player.pitch;
        self.on_ground = move_player.on_ground;
        Ok(())
    }

    fn handle_move_player_rot(&mut self, move_player: MovePlayerRot) -> anyhow::Result<()> {
        if self.pending_teleport.is_some() {
            return Ok(());
        }

        if is_invalid_movement(0.0, 0.0, 0.0, move_player.yaw, move_player.pitch) {
            bail!("invalid move value");
        }

        self.yaw = move_player.yaw;
        self.pitch = move_player.pitch;
        self.on_ground = move_player.on_ground;
        Ok(())
    }
}

fn is_invalid_movement(x: f64, y: f64, z: f64, yaw: f32, pitch: f32) -> bool {
    if !x.is_finite() || !y.is_finite() || !z.is_finite() || 
                !yaw.is_finite() || !pitch.is_finite() {
        return true;
    }

    pitch < -90.0 || pitch > 90.0
}