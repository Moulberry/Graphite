use std::{borrow::Cow, cell::RefCell, collections::VecDeque, f64::consts::E, ops::{ControlFlow, Range}, ptr::NonNull, rc::Rc, time::{Duration, Instant}, u8};

use anyhow::bail;
use downcast_rs::Downcast;
use enum_map::EnumMap;
use enumset::{enum_set_union, EnumSet, EnumSetType};
use graphite_binary::{nbt::EncodedNBT, slice_serialization::SliceSerializable};
use graphite_command::{dispatcher::RootDispatchNode, minecraft::MinecraftRootDispatchNode, types::{ParseErrorType, Span}};
use graphite_mc_constants::{block::BlockAttributes, builtin::{Attribute, DataComponentType, SoundEvent}, entity::{Entity, Metadata, PlayerMetadata}, fluid::Fluid, item::Item, types::{BossBarColor, BossBarOverlay, ClientAction, EntityAnimation, EquipmentSlot, HandAction, InteractionHand, MoveAction, Pose, RelativeMovement, SoundSource}};
use graphite_mc_protocol::{play::{self, clientbound::{AddEntity, AnimateEntity, BlockChangedAck, BundledPacketBuffer, EntityPositionSync, GameEventType, MoveEntityPos, MoveEntityPosRot, MoveEntityRot, PlayerInfoAction, PlayerInfoEntry, RemoveEntities, RotateHead, SetEntityData, TeleportEntity}, serverbound::{AcceptTeleportation, KeepAlive, MovePlayerPos, MovePlayerPosRot, MovePlayerRot, PacketHandler, PlayerHandAction, SetCarriedItem, SetCreativeModeSlot, UseItemOn}}, types::{data_component::*, text::{IntoTextComponent, TextComponent}, BlockHitResult, BlockPosition, ByteRotation, GameProfile, ItemStack, Position, SoundType}, IdentifiedPacket};
use graphite_network::{Connection, FramedPacketHandler, HandleAction, PacketBuffer, SendableConnection};
use glam::{DVec3, IVec3, Vec2, Vec3};
use once_cell::sync::Lazy;
use rand::{Rng, RngCore};
use rustc_hash::FxHashMap;

use crate::{entity::next_entity_id, inventory::{container::Container, inventory_slot::InventorySlot, menu::{BaseMenu, ContainerAction}, player_container_view::PlayerContainerView}, types::AABB, world::{chunk::{ChunkPlayerRef, ChunkProvider}, chunk_view_diff::{self, ChunkDiffStatus}, BlockGetter, GenericWorld, PlayerId, World, WorldExtension}};

use super::{attribute::SyncableAttributeMap, known_client_state::{DebugState, ForcedPose, KnownClientState, KnownClientStateChange, KnownClientStateChangeKey, UncertainVelocity}, language::Language};
pub enum PacketViewability {
    IncludeSelf,
    ExcludeSelf
}

pub trait GenericPlayer: Downcast {
    fn is_still_connected(&self) -> bool;
    fn tick(&mut self);
    fn view_tick(&mut self);
    fn get_packet_buffer(&mut self) -> &mut PacketBuffer;
    fn clear_references(&mut self);
    fn disconnect(&mut self, message: Option<EncodedNBT>);
    fn get_game_profile(&self) -> GameProfile<'static>;
    fn get_uuid(&self) -> u128; // todo: can we remove this?
    fn create_collision_aabb(&self) -> AABB;
    fn get_entity_id(&self) -> i32;
    fn get_player_id(&self) -> PlayerId;
    fn write_add_self_packet(&self, buffer: &mut PacketBuffer);
    fn take_transfer(&mut self) -> Option<(Rc<RefCell<Connection>>, PacketBuffer, Box<dyn FnOnce(SendableConnection)>)>;

}
downcast_rs::impl_downcast!(GenericPlayer);

impl <P: PlayerExtension + 'static> GenericPlayer for Player<P> {
    fn is_still_connected(&self) -> bool {
        if self.pending_transfer.is_some() {
            return false;
        }

        if let Some(connection) = &self.connection {
            !connection.borrow().is_shutdown()
        } else {
            false
        }
    }

    fn tick(&mut self) {
        <Player<P>>::tick(self);
    }

    fn view_tick(&mut self) {
        <Player<P>>::view_tick(self);
    }

    fn get_packet_buffer(&mut self) -> &mut PacketBuffer {
        &mut self.packet_buffer
    }

    fn clear_references(&mut self) {
        if let Some(chunk_ref) = self.chunk_ref.take() {
            let last_chunk_x = (self.last_position.x / 16.0).floor() as i32;
            let last_chunk_z = (self.last_position.z / 16.0).floor() as i32;
            
            let chunk = self.world_mut().get_chunk_mut(last_chunk_x, last_chunk_z).unwrap();
            chunk.remove_player(chunk_ref);
        }
        if let Some(connection) = &self.connection {
            connection.borrow_mut().disconnect_handler();
        }
    }
    
    fn disconnect(&mut self, message: Option<EncodedNBT>) {
        self.pending_transfer = None;

        self.close_menu();
        self.flush_packets();

        if let Some(connection) = self.connection.take() {
            P::before_disconnect(self);

            if let Some(message) = message {
                play::clientbound::Disconnect {
                    message
                }.write_packet(&mut self.packet_buffer);
                connection.borrow_mut().send(&mut self.packet_buffer);
            }

            let mut connection_ref = connection.borrow_mut();
            connection_ref.disconnect_handler();
            connection_ref.shutdown();
        }
    }

    fn get_game_profile(&self) -> GameProfile<'static> {
        self.profile.clone()
    }

    fn get_uuid(&self) -> u128 {
        self.profile.uuid
    }

    fn create_collision_aabb(&self) -> AABB {
        self.create_collision_aabb_for_pose(self.known_client_state.pose)
    }

    fn get_entity_id(&self) -> i32 {
        self.entity_id
    }

    fn get_player_id(&self) -> PlayerId {
        self.player_id
    }

    fn write_add_self_packet(&self, buffer: &mut PacketBuffer) {
        AddEntity {
            id: self.entity_id,
            uuid: self.profile.uuid,
            entity_type: Entity::Player as i32,
            x: self.position.x,
            y: self.position.y,
            z: self.position.z,
            pitch: self.pitch,
            yaw: self.yaw,
            head_yaw: self.yaw,
            data: 0,
            x_vel: 0.0,
            y_vel: 0.0,
            z_vel: 0.0,
        }.write_packet(buffer);

        SetEntityData::write_non_default(&self.metadata, self.entity_id, buffer);

        // Write equipment
        let mut changed_equipment = Vec::new();
        for equipment_index in 0..6 {
            let item_stack = self.stripped_visible_equipment[equipment_index].clone();
            if !item_stack.is_empty() {
                changed_equipment.push((EquipmentSlot::try_from(equipment_index as u8).unwrap(), item_stack));        
            }
        }
        if !changed_equipment.is_empty() {
            play::clientbound::SetEquipment {
                entity_id: self.entity_id,
                equipment: changed_equipment,
            }.write_packet(buffer);
        }
    }

    fn take_transfer(&mut self) -> Option<(Rc<RefCell<Connection>>, PacketBuffer, Box<dyn FnOnce(SendableConnection)>)> {
        let transfer = self.pending_transfer.take()?;

        self.close_menu();
        self.flush_packets();

        let extra = P::get_extra_transfer_data(self)?;
        let connection = self.connection.take()?;

        let packet_buffer = std::mem::replace(&mut self.packet_buffer, PacketBuffer::new());

        return Some((connection, packet_buffer, Box::new(|connection| {
            (transfer)(connection, extra);
        })));
    }
}

pub enum TickUsingItemResult {
    Continue,
    Finish,
    Abort
}

pub enum UseItemResult {
    Nothing,
    UsedItem,
    StartUsingItem
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DropCause {
    Hotbar,
    Inventory,
    InventoryOutside,
    ContainerClosed,
    Custom(i32)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SwingCause {
    DestroyingBlock, // Not implemented
    Miss, // Fallback swing cause when no other cases match
    InteractWithEntity, // After InteractEntity
    InteractWithBlock, // After UseItemOn
    UseItem, // After UseItem
    Drop, // After dropping
}

#[allow(unused_variables)]
pub trait PlayerExtension: Sized + 'static {
    type World: WorldExtension;
    type InventoryContainer: Container;
    type Menu: BaseMenu<Self>;
    type ExtraTransferData;

    const REACH_THRESHOLD: f64 = 3.0;

    fn tick(player: &mut Player<Self>) {
    }

    fn initialize_commands(player: &mut Player<Self>) -> Option<MinecraftRootDispatchNode<Player<Self>>> {
        None
    }

    fn interact_entity_at(player: &mut Player<Self>, entity: hecs::Entity, hand: InteractionHand, pos: Vec3, distance: f64) -> bool {
        false
    }

    fn interact_with_unknown_entity(player: &mut Player<Self>, entity_id: i32, hand: InteractionHand) -> bool {
        false
    }

    fn attack_unknown_entity(player: &mut Player<Self>, entity_id: i32) {
    }

    fn tick_using_item(player: &mut Player<Self>, slot: InventorySlot, ticks: usize) -> TickUsingItemResult {
        TickUsingItemResult::Abort
    }

    fn use_item(player: &mut Player<Self>, hand: InteractionHand, on_block: Option<BlockHitResult>) -> UseItemResult {
        UseItemResult::Nothing
    }

    fn on_inventory_clicked(player: &mut Player<Self>, action: ContainerAction) -> bool {
        false
    }

    fn client_action(player: &mut Player<Self>, action: ClientAction) {
    }

    fn finish_using_item(player: &mut Player<Self>, slot: InventorySlot, ticks: usize) {
    }

    fn abort_using_item(player: &mut Player<Self>) {
    }

    fn attack_strength_reset(player: &mut Player<Self>) {
    }

    fn swap_item_with_off_hand(player: &mut Player<Self>) {
    }

    fn drop_item_in_hand(player: &mut Player<Self>, all: bool) {
    }

    fn set_creative_mode_slot(player: &mut Player<Self>, slot: i16, item_stack: Option<&ItemStack>) {
    }

    fn attack_entity(player: &mut Player<Self>, entity: hecs::Entity, distance: f64) {
    }

    fn swing(player: &mut Player<Self>, swing_cause: SwingCause) {
    }

    fn land_after_falling(player: &mut Player<Self>, distance: f32) {
    }

    fn drop_item(player: &mut Player<Self>, item: <Self::InventoryContainer as Container>::Item, cause: DropCause) {
    }

    fn before_disconnect(player: &mut Player<Self>) {
    }

    fn handle_ping_pong_response(player: &mut Player<Self>, id: i32) {
    }

    fn handle_place_recipe(player: &mut Player<Self>, recipe: play::serverbound::PlaceRecipe) {
    }

    fn get_extra_transfer_data(player: &mut Player<Self>) -> Option<Self::ExtraTransferData>;
}

pub struct Player<P: PlayerExtension> {
    world: NonNull<World<P::World>>,
    pub player_id: PlayerId,

    pub(crate) connection: Option<Rc<RefCell<Connection>>>,
    pub(crate) pending_transfer: Option<Box<dyn FnOnce(SendableConnection, P::ExtraTransferData)>>,
    pub packet_buffer: PacketBuffer,
    include_self_packet_buffer: PacketBuffer,
    exclude_self_packet_buffer: PacketBuffer,
    exclude_self_viewable_range: Option<Range<usize>>,
    pub entity_id: i32,

    pub profile: GameProfile<'static>,
    pub language: Language,

    // General fields
    last_position: DVec3,
    pub position: DVec3,
    pub on_ground: bool,
    pub yaw: f32,
    pub pitch: f32,
    pub attributes: SyncableAttributeMap,
    pub(crate) chunk_ref: Option<ChunkPlayerRef>,

    pub hotbar_slot: u8,
    pub metadata: PlayerMetadata,
    pub force_pose: Option<Pose>,
    pub attack_strength_ticker: usize,
    pub(super) held_item_type: Item,
    pub(super) fall_distance: f32,

    pub forwards_input: i8,
    pub strafe_input: i8,
    pub jump_pressed: bool,
    pub shift_pressed: bool,

    pub camera_entity_id: i32,

    pub container_view: PlayerContainerView<P>,
    pub raw_visible_equipment: [ItemStack; 6],
    pub stripped_visible_equipment: [ItemStack; 6],

    pub fluid_at_eyes: Fluid,
    pub fluid_heights: FluidHeights,

    // Abilities
    ability_flags: u8,
    flying_speed: f32,
    walking_speed: f32,
    abilities_dirty: bool,

    // Fields for packet handling
    pub(super) packets_handled_this_tick: EnumSet<PacketHandledThisTick>,
    pub(super) player_loaded_time: u8,
    pub(super) ack_sequence_up_to: Option<i32>,
    pub(super) keep_alive_timer: u8,
    pub(super) current_keep_alive: u64,
    pub(super) using_item: Option<UsingItem>,
    pub(super) attacked_entity: bool,
    pub(super) swing_time: u8,
    pub(super) last_client_tick: Instant,
    pub(super) client_tick_nanos_pool: u64,
    pub(super) last_client_position: DVec3,
    pub(super) last_client_on_ground: bool,
    pub(super) last_client_horizontal_collision: bool,
    pub(super) shift_pressed_last_tick: bool,
    pub(super) sprinting_last_tick: bool,
    pub(super) no_jump_delay: u8,
    pub(super) stuck_multiplier: DVec3,
    pub(super) main_supporting_block: Option<IVec3>,
    next_remote_state_sync_id: u16,
    known_client_state_changes: Vec<(u8, KnownClientStateChangeKey, Vec<KnownClientStateChange>)>,
    pub(super) known_client_state: KnownClientState,
    pub(super) last_entity_collision_counts: VecDeque<(u8, u8)>,
    pub(super) last_swing_cause: SwingCause,
    pub(super) pending_swing_hand: Option<InteractionHand>,

    pub(super) commands: Option<RootDispatchNode<Self>>,

    synced_position: Option<DVec3>,
    old_rotation: (u8, u8),
    position_sync_time: usize,

    // Extension
    pub extension: P
}

impl <P: PlayerExtension> Player<P> {
    pub fn new(world: NonNull<World<P::World>>, player_id: PlayerId, position: DVec3, connection: Rc<RefCell<Connection>>,
            profile: GameProfile<'static>, inventory: P::InventoryContainer, extension: P) -> Self {
        let entity_id = next_entity_id();
        let known_client_state = KnownClientState {
            awaiting_absolute_teleport_count: 0,
            debug_state: None,
            pose: Pose::Standing,
            forced_pose: ForcedPose::Standing,
            velocity: UncertainVelocity::default(),
            living_flags: 0,
            use_item: Item::Air,
            held_item_stack: ItemStack::EMPTY,
            shared_flags: 0,
            is_flying: false,
            is_flying_pre_tick: false,
            can_fly: false,
            flying_speed: 0.05,
            sneaking_speed: Attribute::SneakingSpeed.default_value(),
            applied_sprint_modifier_to_movement_speed: false,
            movement_speed: Attribute::MovementSpeed.default_value(),
            jump_strength: Attribute::JumpStrength.default_value(),
            gravity: Attribute::Gravity.default_value(),
            step_height: Attribute::StepHeight.default_value(),
            water_movement_efficiency: Attribute::WaterMovementEfficiency.default_value(),
            effects: EnumMap::default(),
            known_blocks: FxHashMap::default()
        };
        let mut attributes = SyncableAttributeMap::new();
        attributes.set_base_value(Attribute::AttackDamage, 1.0);
        attributes.set_base_value(Attribute::MovementSpeed, 0.1);
        attributes.set_base_value(Attribute::BlockInteractionRange, 4.5);
        attributes.set_base_value(Attribute::EntityInteractionRange, 3.0);
        Self {
            world,
            player_id,

            connection: Some(connection),
            pending_transfer: None,
            packet_buffer: PacketBuffer::new(),
            include_self_packet_buffer: PacketBuffer::new(),
            exclude_self_packet_buffer: PacketBuffer::new(),
            exclude_self_viewable_range: None,
            entity_id,

            profile,
            language: Language::default(),

            last_position: position,
            position,
            on_ground: false,
            yaw: 0.0,
            pitch: 0.0,
            attributes,
            chunk_ref: None,

            hotbar_slot: 0,
            metadata: PlayerMetadata::default(),
            force_pose: None,
            attack_strength_ticker: 0,
            held_item_type: Item::Air,
            fall_distance: 0.0,

            forwards_input: 0,
            strafe_input: 0,
            jump_pressed: false,
            shift_pressed: false,
            shift_pressed_last_tick: false,
            sprinting_last_tick: false,
            no_jump_delay: 0,
            stuck_multiplier: DVec3::ZERO,
            main_supporting_block: None,

            camera_entity_id: entity_id,

            container_view: PlayerContainerView::new(inventory),
            raw_visible_equipment: [ItemStack::EMPTY; 6],
            stripped_visible_equipment: [ItemStack::EMPTY; 6],

            fluid_at_eyes: Fluid::Empty,
            fluid_heights: FluidHeights::default(),

            ability_flags: 0,
            flying_speed: 0.05,
            walking_speed: 0.1,
            abilities_dirty: false,

            packets_handled_this_tick: EnumSet::empty(),
            player_loaded_time: 60,
            ack_sequence_up_to: None,
            keep_alive_timer: 0,
            current_keep_alive: 0,
            using_item: None,
            attacked_entity: false,
            swing_time: 0,
            last_client_tick: Instant::now(),
            client_tick_nanos_pool: (Duration::from_millis(50) * 256).as_nanos() as u64,
            last_client_position: DVec3::ZERO,
            last_client_on_ground: false,
            last_client_horizontal_collision: false,
            next_remote_state_sync_id: 0,
            known_client_state_changes: Vec::new(),
            known_client_state,
            last_entity_collision_counts: VecDeque::with_capacity(8),
            last_swing_cause: SwingCause::Miss,
            pending_swing_hand: None,

            commands: None,

            synced_position: None,
            old_rotation: (0, 0),
            position_sync_time: 0,

            extension
        }
    }

    pub fn update_commands(&mut self) {
        let commands = P::initialize_commands(self);

        if let Some(commands) = commands {
            let (dispatcher, packet) =
                graphite_command::minecraft::create_dispatcher_and_brigadier_packet(commands);

            self.commands = Some(dispatcher);
            self.send_packet(&packet);
        } else {
            self.commands = None;
        }
    }

    pub fn get_on_pos(&self) -> IVec3 {
        super::packet_handling::get_on_pos_legacy(self.main_supporting_block, self.position)
    }

    pub fn allow_world_interaction(&self) -> bool {
        !self.is_sleeping() && self.entity_id == self.camera_entity_id && self.container_view.get_container_type().is_none()
    }

    pub fn set_camera_entity(&mut self, camera_id: Option<i32>) {
        let camera_id = camera_id.unwrap_or(self.entity_id);

        let old_camera_entity_id = self.camera_entity_id;
        if old_camera_entity_id == camera_id {
            return;
        }
        self.camera_entity_id = camera_id;

        if camera_id == self.entity_id {
            let profile = self.profile.clone();
            let mut bundle = BundledPacketBuffer::new(&mut self.packet_buffer);

            bundle.write(&play::clientbound::GameEvent {
                event_type: GameEventType::ChangeGameMode,
                param: 0.0,
            });
            bundle.write(&play::clientbound::PlayerInfoUpdate {
                actions: EnumSet::only(PlayerInfoAction::UpdateGameMode),
                entries: vec![
                    PlayerInfoEntry {
                        profile,
                        listed: false,
                        latency: 0,
                        gamemode: 0,
                        display_name: None,
                    }
                ],
            });
            self.abilities_dirty = true;
            
            bundle.write(&play::clientbound::SetCamera {
                entity_id: camera_id,
            });

            bundle.finish();
            return;
        }

        let was_spectating_other = old_camera_entity_id != self.entity_id;
        let is_spectating_other = camera_id != self.entity_id;

        if was_spectating_other == is_spectating_other {
            play::clientbound::SetCamera {
                entity_id: camera_id,
            }.write_packet(&mut self.packet_buffer);
            return;
        }

        let profile = self.profile.clone();
        let mut bundle = BundledPacketBuffer::new(&mut self.packet_buffer);

        bundle.write(&play::clientbound::GameEvent {
            event_type: GameEventType::ChangeGameMode,
            param: 3.0,
        });
        bundle.write(&play::clientbound::PlayerInfoUpdate {
            actions: EnumSet::only(PlayerInfoAction::UpdateGameMode),
            entries: vec![
                PlayerInfoEntry {
                    profile,
                    listed: false,
                    latency: 0,
                    gamemode: 3,
                    display_name: None,
                }
            ],
        });
        self.abilities_dirty = true;

        bundle.write(&play::clientbound::SetCamera {
            entity_id: camera_id,
        });

        bundle.finish();
    }

    pub fn open_menu(&mut self, menu: P::Menu) {
        let function = self.container_view.close(&mut self.packet_buffer, false);
        if let Some(function) = function {
            (function)(self);
        }

        if self.container_view.get_menu().is_some() {
            panic!("Menu was reopened after being closed by the server. New menu can't be opened now")
        }

        self.container_view.open_menu(menu, &mut self.packet_buffer);
    }

    pub fn close_menu(&mut self) {
        if let Some(held) = self.container_view.drop_held_item() {
            P::drop_item(self, held, DropCause::ContainerClosed);
        }

        let function = self.container_view.close(&mut self.packet_buffer, true);
        if let Some(function) = function {
            (function)(self);
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

    pub fn eye_position(&self) -> DVec3 {
        let offset = match self.known_client_state.pose {
            Pose::FallFlying => 0.4,
            Pose::Sleeping => 0.2,
            Pose::Swimming => 0.4,
            Pose::SpinAttack => 0.4,
            Pose::Crouching => 1.27,
            _ => 1.62,
        };

        self.position + DVec3::new(0.0, offset, 0.0)
    }

    pub fn is_using_item(&self) -> bool {
        return self.using_item.is_some();
    }

    pub fn play_sound(&mut self, sound: SoundType<'_>) {
        let eye = self.eye_position();
        play::clientbound::Sound {
            sound,
            source: SoundSource::Master,
            x: eye.x as f32,
            y: eye.y as f32,
            z: eye.z as f32,
            volume: 1.0,
            pitch: 1.0,
            seed: rand::thread_rng().gen(),
        }.write_packet(&mut self.packet_buffer);
    }

    pub fn send_packet<'r, 'd: 'r, I: std::fmt::Debug, T>(&mut self, packet: &'r T)
    where
        T: SliceSerializable<'r, 'd, T> + IdentifiedPacket<I> + 'd,
    {
        packet.write_packet(&mut self.packet_buffer)
    }

    pub fn add_viewable_packet<'r, 'd: 'r, I: std::fmt::Debug, T>(&mut self, packet: &'r T, viewability: PacketViewability)
    where
        T: SliceSerializable<'r, 'd, T> + IdentifiedPacket<I> + 'd,
    {
        match viewability {
            PacketViewability::IncludeSelf => {
                packet.write_packet(&mut self.include_self_packet_buffer);
            },
            PacketViewability::ExcludeSelf => {
                packet.write_packet(&mut self.exclude_self_packet_buffer);
            },
        }
    }

    pub fn get_expected_velocity(&self) -> DVec3 {
        self.known_client_state.velocity.get_expected()
    }

    pub fn send_velocity(&mut self, velocity: DVec3) {
        let relative_arguments = RelativeMovement::POSITION | RelativeMovement::ROTATION;
        self.teleport_full(0.0, 0.0, DVec3::ZERO, velocity, relative_arguments);
    }

    pub fn send_add_velocity(&mut self, velocity: DVec3, reset_y: bool) {
        let mut relative_arguments = RelativeMovement::POSITION | RelativeMovement::ROTATION | RelativeMovement::DeltaX | RelativeMovement::DeltaZ;
        if !reset_y {
            relative_arguments |= RelativeMovement::DeltaY;
        }
        self.teleport_full(0.0, 0.0, DVec3::ZERO, velocity, relative_arguments);
    }

    pub fn is_flying(&self) -> bool {
        (self.ability_flags & (1 << 1)) != 0
    }

    pub fn set_flying(&mut self, flying: bool) {
        if flying == self.is_flying() {
            return;
        }

        self.set_flying_raw(flying);
        self.abilities_dirty = true;
    }

    pub(crate) fn set_flying_raw(&mut self, flying: bool) {
        if flying {
            self.ability_flags |= 1 << 1;
        } else {
            self.ability_flags &= !(1 << 1);
        }
    }

    pub fn can_fly(&self) -> bool {
        (self.ability_flags & (1 << 2)) != 0
    }

    pub fn set_can_fly(&mut self, can_fly: bool) {
        if can_fly == self.can_fly() {
            return;
        }

        if can_fly {
            self.ability_flags |= 1 << 2;
        } else {
            self.ability_flags &= !(1 << 2);
        }
        self.abilities_dirty = true;
    }

    pub fn set_flying_speed(&mut self, flying_speed: f32) {
        if self.flying_speed != flying_speed {
            self.flying_speed = flying_speed;
            self.abilities_dirty = true;
        }
    }

    pub fn set_walking_speed(&mut self, walking_speed: f32) {
        if self.walking_speed != walking_speed {
            self.walking_speed = walking_speed;
            self.abilities_dirty = true;
        }
    }

    pub fn get_shared_flag(&self, flag: u8) -> bool {
        (self.metadata.shared_flags & (1 << flag)) != 0
    }

    pub fn set_shared_flag(&mut self, flag: u8, value: bool) {
        let new_shared_flags = if value {
            self.metadata.shared_flags | (1 << flag)
        } else {
            self.metadata.shared_flags & !(1 << flag)
        };
        if self.metadata.shared_flags != new_shared_flags {
            self.metadata.set_shared_flags(new_shared_flags);
        }
    }

    pub fn is_on_fire(&self) -> bool {
        self.get_shared_flag(0)
    }

    pub fn set_on_fire(&mut self, value: bool) {
        self.set_shared_flag(0, value)
    }

    pub fn is_sneaking(&self) -> bool {
        self.get_shared_flag(1)
    }

    pub(super) fn set_sneaking(&mut self, value: bool) {
        self.set_shared_flag(1, value)
    }

    pub fn is_sprinting(&self) -> bool {
        self.get_shared_flag(3)
    }

    pub(super) fn set_sprinting_from_client(&mut self, value: bool) {
        self.attributes.set_modifier_sprinting(value);
        self.set_shared_flag(3, value);

        if value {
            self.known_client_state.shared_flags |= 1 << 3;
    
            if !self.known_client_state.applied_sprint_modifier_to_movement_speed {
                self.known_client_state.applied_sprint_modifier_to_movement_speed = true;
                self.known_client_state.movement_speed *= 1.3;
            }
        } else {
            self.known_client_state.shared_flags &= !(1 << 3);

            if self.known_client_state.applied_sprint_modifier_to_movement_speed {
                self.known_client_state.applied_sprint_modifier_to_movement_speed = false;
                self.known_client_state.movement_speed /= 1.3;
            }
        }
    }

    pub fn is_swimming(&self) -> bool {
        self.get_shared_flag(4)
    }

    pub(crate) fn set_swimming(&mut self, value: bool) {
        self.set_shared_flag(4, value)
    }

    pub fn is_invisible(&self) -> bool {
        self.get_shared_flag(5)
    }

    pub fn set_invisible(&mut self, value: bool) {
        self.set_shared_flag(5, value)
    }

    pub fn is_glowing(&self) -> bool {
        self.get_shared_flag(6)
    }

    pub fn set_glowing(&mut self, value: bool) {
        self.set_shared_flag(6, value)
    }

    pub fn is_fall_flying(&self) -> bool {
        self.get_shared_flag(7)
    }

    pub fn set_fall_flying(&mut self, value: bool) {
        self.set_shared_flag(7, value)
    }

    pub fn is_auto_spin_attack(&self) -> bool {
        (self.metadata.living_entity_flags & (1 << 2)) != 0
    }

    pub fn set_auto_spin_attack(&mut self, value: bool) {
        if value {
            self.metadata.set_living_entity_flags(self.metadata.living_entity_flags | (1 << 2));
        } else {
            self.metadata.set_living_entity_flags(self.metadata.living_entity_flags & !(1 << 2));
        }
    }

    pub fn is_sleeping(&self) -> bool {
        self.metadata.sleeping_pos.is_some()
    }

    pub fn set_sleeping_pos(&mut self, pos: Option<IVec3>) {
        if let Some(pos) = pos {
            let pos = graphite_mc_protocol::types::BlockPosition::new(pos.x, pos.y, pos.z);
            let pos = Some(pos.encode());
            if self.metadata.sleeping_pos != pos {
                self.metadata.set_sleeping_pos(pos);
            }
        } else if self.metadata.sleeping_pos != None {
            play::clientbound::AnimateEntity {
                entity_id: self.entity_id,
                animation: EntityAnimation::WakeUp,
            }.write_packet(&mut self.packet_buffer);
            self.teleport_full(self.yaw, self.pitch, self.position, DVec3::ZERO, EnumSet::empty());

            self.metadata.set_sleeping_pos(None);
        }
    }

    pub fn teleport_rotation(&mut self, yaw: f32, pitch: f32) {
        self.teleport_full(yaw, pitch, DVec3::ZERO, DVec3::ZERO, RelativeMovement::POSITION | RelativeMovement::VELOCITY)
    }

    pub fn teleport_position(&mut self, position: DVec3) {
        self.teleport_full(0.0, 0.0, position, DVec3::ZERO, RelativeMovement::ROTATION)
    }

    pub fn teleport_position_and_rotation(&mut self, position: DVec3, yaw: f32, pitch: f32) {
        self.teleport_full(yaw, pitch, position, DVec3::ZERO, EnumSet::empty())
    }

    pub fn teleport_position_and_velocity(&mut self, position: DVec3, velocity: DVec3) {
        self.teleport_full(0.0, 0.0, position, velocity, RelativeMovement::ROTATION)
    }

    pub fn teleport_full(&mut self, yaw: f32, pitch: f32, position: DVec3, velocity: DVec3, relative_arguments: EnumSet<RelativeMovement>) {
        let teleport_id = rand::random();

        if crate::debug::CHECK_NAN_OR_INF {
            if !yaw.is_finite() || !pitch.is_finite() || !position.is_finite() || !velocity.is_finite() {
                panic!("Teleport contains nan or inf: yaw={:?}, pitch={:?}, position={:?}, velocity={:?}", yaw, pitch, position, velocity)
            }
        }

        play::clientbound::PlayerPosition {
            teleport_id,
            x: position.x,
            y: position.y,
            z: position.z,
            x_vel: velocity.x,
            y_vel: velocity.y,
            z_vel: velocity.z,
            yaw,
            pitch,
            relative_arguments: relative_arguments.as_u32() as i32,
        }.write_packet(&mut self.packet_buffer);

        let mut changes = Vec::new();

        // Skip if relative position change with all zeros
        if !relative_arguments.is_superset(RelativeMovement::POSITION) || position != DVec3::ZERO {
            let mut flags = 0;
            if relative_arguments.contains(RelativeMovement::X) {
                flags |= 1;
            }
            if relative_arguments.contains(RelativeMovement::Y) {
                flags |= 2;
            }
            if relative_arguments.contains(RelativeMovement::Z) {
                flags |= 4;
            }
            
            changes.push(KnownClientStateChange::TeleportPosition {
                position,
                relative_coords: flags
            });

            // Increase absolute count if performing an absolute teleport
            if flags == 0 {
                self.position = position;
                if self.known_client_state.awaiting_absolute_teleport_count == u8::MAX {
                    self.disconnect(None);
                } else {
                    self.known_client_state.awaiting_absolute_teleport_count += 1;
                }
            }
        }

        // Skip if relative velocity change with all zeros
        if !relative_arguments.is_superset(RelativeMovement::VELOCITY) || velocity != DVec3::ZERO {
            let mut flags = 0;
            if relative_arguments.contains(RelativeMovement::DeltaX) {
                flags |= 1;
            }
            if relative_arguments.contains(RelativeMovement::DeltaY) {
                flags |= 2;
            }
            if relative_arguments.contains(RelativeMovement::DeltaZ) {
                flags |= 4;
            }
            
            changes.push(KnownClientStateChange::TeleportVelocity {
                velocity,
                relative_coords: flags
            });
        }

        if !changes.is_empty() {
            self.known_client_state_changes.push((self.keep_alive_timer, KnownClientStateChangeKey::TeleportId(teleport_id), changes));
        }
    }

    pub fn send_damage_without_source_entity(&mut self, source_type_id: i32, source_position: Option<DVec3>, hurt_angle: Option<f32>) {
        let hurt_angle = if let Some(hurt_angle) = hurt_angle {
            hurt_angle - self.yaw
        } else if let Some(source_position) = source_position {
            let dx = source_position.x - self.position.x;
            let dz = source_position.z - self.position.z;
            dz.atan2(dx).to_degrees() as f32 - self.yaw
        } else {
            0.0
        };

        play::clientbound::HurtAnimation {
            entity_id: self.entity_id,
            hurt_angle,
        }.write_packet(&mut self.packet_buffer);

        play::clientbound::DamageEvent {
            entity_id: self.entity_id,
            source_type_id,
            source_cause_id: 0,
            source_direct_id: 0,
            source_position: source_position.map(|v| Position { x: v.x, y: v.y, z: v.z })
        }.write_packet(&mut self.packet_buffer);
    }

    pub fn send_chat_message<'a>(&mut self, message: impl IntoTextComponent<'a>) {
        play::clientbound::SystemChat {
            message: message.into_text_component().to_encoded_nbt(),
            overlay: false,
        }.write_packet(&mut self.packet_buffer);
    }

    pub fn send_action_bar_message<'a>(&mut self, message: impl IntoTextComponent<'a>) {
        play::clientbound::SystemChat {
            message: message.into_text_component().to_encoded_nbt(),
            overlay: true,
        }.write_packet(&mut self.packet_buffer);
    }

    pub fn send_add_boss_bar<'a>(&mut self, uuid: u128, title: impl IntoTextComponent<'a>, health: f32, color: BossBarColor,
            division: BossBarOverlay, flags: u8) {
        play::clientbound::BossEvent {
            uuid,
            action: play::clientbound::BossEventAction::Add {
                title: title.into_text_component().to_encoded_nbt(),
                health,
                color,
                division,
                flags
            },
        }.write_packet(&mut self.packet_buffer);
    }

    pub fn send_remove_boss_bar<'a>(&mut self, uuid: u128) {
        play::clientbound::BossEvent {
            uuid,
            action: play::clientbound::BossEventAction::Remove {},
        }.write_packet(&mut self.packet_buffer);
    }

    pub fn set_experience(&mut self, progress: f32, level: i32) {
        play::clientbound::SetExperience {
            progress,
            level,
            total: 0,
        }.write_packet(&mut self.packet_buffer);
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

    pub fn create_collision_aabb_at(&self, position: DVec3) -> AABB {
        self.create_collision_aabb_for_pose_at(self.known_client_state.pose, position)
    }

    pub fn create_collision_aabb_for_pose(&self, pose: Pose) -> AABB {
        self.create_collision_aabb_for_pose_at(pose, self.position)
    }

    pub fn create_collision_aabb_for_pose_at(&self, pose: Pose, position: DVec3) -> AABB {
        let (width, height) = match pose {
            Pose::Sleeping | Pose::Dying => (0.2_f32, 0.2_f32),
            Pose::FallFlying | Pose::Swimming | Pose::SpinAttack => (0.6_f32, 0.6_f32),
            Pose::Crouching => (0.6_f32, 1.5_f32),
            _ => (0.6_f32, 1.8_f32)
        };

        let half_width = width / 2.0_f32;

        let min = DVec3::new(position.x - half_width as f64, position.y, position.z - half_width as f64);
        let max = DVec3::new(position.x + half_width as f64, position.y + height as f64, position.z + half_width as f64);
        AABB::new(min, max)
    }

    pub fn transfer(&mut self, function: Box<dyn FnOnce(SendableConnection, P::ExtraTransferData)>) {
        self.flush_packets();
        self.pending_transfer = Some(function);
    }

    pub fn flush_packets(&mut self) {
        if self.pending_transfer.is_some() {
            self.packet_buffer.clear();
            return;
        }

        if let Some(connection) = &self.connection {
            connection.borrow_mut().send(&mut self.packet_buffer)
        } else {
            self.packet_buffer.clear();
        }
    }

    fn tick(&mut self) {
        if !self.is_still_connected() {
            self.disconnect(Some(TextComponent::literal("Disconnected").to_encoded_nbt()));
            return;
        }

        P::tick(self);

        if !self.is_still_connected() {
            if self.pending_transfer.is_none() {
                self.disconnect(Some(TextComponent::literal("Disconnected").to_encoded_nbt()));
            }
            return;
        }

        // Send keep alive timer
        self.keep_alive_timer = self.keep_alive_timer.wrapping_add(1);
        if self.keep_alive_timer == 0 {
            if self.current_keep_alive != 0 {
                self.disconnect(Some(TextComponent::literal("Timed out").to_encoded_nbt()));
                return;
            }
            self.current_keep_alive = rand::thread_rng().next_u64();

            self.send_packet(&play::clientbound::KeepAlive {
                id: self.current_keep_alive,
            });
        }

        // Check for client not accepting state changes in time
        if let Some(&(timeout, _, _)) = self.known_client_state_changes.first() {
            if timeout == self.keep_alive_timer { // This means that the timer has wrapped back around, so 256 ticks have passed
                self.disconnect(Some(TextComponent::literal("Timed out (state change)").to_encoded_nbt()));
                return;
            }
        }

        // Reset trackers
        self.attacked_entity = false;

        // Update attack strength ticker
        let hotbar_slot = InventorySlot::Hotbar(self.hotbar_slot);
        
        let held_item = self.container_view.get_inventory_item_stack(hotbar_slot).item;
        if held_item != self.held_item_type {
            self.held_item_type = held_item;
            if self.attack_strength_ticker > 0 {
                self.attack_strength_ticker = 0;
                P::attack_strength_reset(self);
            }
        } else {
            self.attack_strength_ticker = self.attack_strength_ticker.saturating_add(1);
        }

        // Update swing time
        if self.swing_time > 0 {
            self.swing_time -= 1;
        }

        // Update using item
        if let Some(using_item) = &mut self.using_item {
            using_item.ticks += 1;

            let (using_slot, active_item) = match using_item.hand {
                InteractionHand::MainHand => (hotbar_slot, held_item),
                InteractionHand::OffHand => (InventorySlot::OffHand, self.container_view.get_inventory_item_stack(InventorySlot::OffHand).item),
            };

            if using_item.slot == using_slot && using_item.item_stack.item == active_item {
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

        // Handle dropped items
        // todo:
        // if !self.inventory.dropped_items.is_empty() {
        //     let mut dropped = Vec::with_capacity(self.inventory.dropped_items.len());
        //     for item in self.inventory.dropped_items.drain(..) {
        //         dropped.push(item);
        //     }
        //     for item in dropped {
        //         P::drop_item(self, item);
        //     }
        // }

        // Synchronize inventory
        self.container_view.synchronize(&mut self.packet_buffer);

        // Synchronize metadata for viewers
        if let Some(force_pose) = self.force_pose {
            self.metadata.set_pose(force_pose);
        }
        SetEntityData::write_changes_without_clearing(&mut self.metadata, self.entity_id, &mut self.exclude_self_packet_buffer);

        // Synchronize equipment to other players
        let mut changed_equipment = Vec::new();
        for equipment_index in 0..6 {
            let slot = match equipment_index {
                0 => InventorySlot::Hotbar(self.hotbar_slot),
                1 => InventorySlot::OffHand,
                2 => InventorySlot::Feet,
                3 => InventorySlot::Legs,
                4 => InventorySlot::Chest,
                5 => InventorySlot::Head,
                _ => unreachable!()
            };

            let item_stack = self.container_view.inventory.get_item_stack(slot).unwrap_or_default();
            if item_stack != self.raw_visible_equipment[equipment_index] {
                self.raw_visible_equipment[equipment_index] = item_stack.clone();

                let mut stripped = item_stack;
                stripped.components.retain_only(enum_set_union!(
                    DataComponentType::MaxDamage,
                    DataComponentType::Damage,
                    DataComponentType::ItemModel,
                    DataComponentType::CustomModelData,
                    DataComponentType::EnchantmentGlintOverride,
                    DataComponentType::DyedColor,
                    DataComponentType::Trim,
                    DataComponentType::Profile,
                    DataComponentType::BannerPatterns,
                    DataComponentType::BaseColor,
                    DataComponentType::PotDecorations,
                ));

                if stripped != self.stripped_visible_equipment[equipment_index] {
                    self.stripped_visible_equipment[equipment_index] = stripped.clone();
                    changed_equipment.push((EquipmentSlot::try_from(equipment_index as u8).unwrap(), stripped));
                }
            }            
        }
        if !changed_equipment.is_empty() {
            play::clientbound::SetEquipment {
                entity_id: self.entity_id,
                equipment: changed_equipment,
            }.write_packet(&mut self.exclude_self_packet_buffer);
        }

        // Synchronize changes to the KnownPlayerState
        let is_flying = self.is_flying();
        let allow_flying = self.can_fly();
        let mut known_state_changes = Vec::new();

        let mut bundle = BundledPacketBuffer::new(&mut self.packet_buffer);

        let hotbar_slot = InventorySlot::Hotbar(self.hotbar_slot);
        let held_item_stack = self.container_view.get_inventory_item_stack(hotbar_slot);

        // Synchronize entity metadata to self
        if self.metadata.is_living_entity_flags_changed() {
            // todo: don't send if known_client_state's flags match the metadata's flags AND we don't have any pending state changes that affect the living flags
            // this also means we need a counter for the number of living flag changes

            let use_item = if (self.metadata.living_entity_flags & 1) != 0 {
                held_item_stack.item
            } else {
                Item::Air
            };
            known_state_changes.push(KnownClientStateChange::SetLivingFlags {
                flags: self.metadata.living_entity_flags,
                use_item
            });
        }
        if held_item_stack != self.known_client_state.held_item_stack {
            known_state_changes.push(KnownClientStateChange::SetHeldItemStack {
                held_item_stack
            });
        }

        let mut sent_shared_flags = false;
        if self.metadata.is_shared_flags_changed() {
            known_state_changes.push(KnownClientStateChange::SetSharedFlags(self.metadata.shared_flags));
            sent_shared_flags = true;
        }
        if let Some(force_pose) = self.force_pose.take() {
            known_state_changes.push(KnownClientStateChange::SetPose(force_pose));
        } else {
            self.metadata.unmark_changes_to_pose();
        }
        SetEntityData::write_changes(&mut self.metadata, self.entity_id, &mut *bundle);

        // Synchronize attributes
        let attribute_changes = self.attributes.process_changes(sent_shared_flags);
        if !attribute_changes.is_empty() {
            for change in &attribute_changes {
                match change.id {
                    Attribute::MovementSpeed => {
                        known_state_changes.push(KnownClientStateChange::SetMovementSpeed {
                            speed: self.attributes.get(Attribute::MovementSpeed),
                            sprint_modifier: self.attributes.get_modifier_sprinting(),
                        });
                    },
                    Attribute::JumpStrength => {
                        known_state_changes.push(KnownClientStateChange::SetJumpStrength(self.attributes.get(Attribute::JumpStrength)));
                    },
                    Attribute::SneakingSpeed => {
                        known_state_changes.push(KnownClientStateChange::SetSneakingSpeed(self.attributes.get(Attribute::SneakingSpeed)));
                    },
                    Attribute::Gravity => {
                        known_state_changes.push(KnownClientStateChange::SetGravity(self.attributes.get(Attribute::Gravity)));
                    },
                    Attribute::StepHeight => {
                        known_state_changes.push(KnownClientStateChange::SetStepHeight(self.attributes.get(Attribute::StepHeight)));
                    },
                    Attribute::WaterMovementEfficiency => {
                        known_state_changes.push(KnownClientStateChange::SetWaterMovementEfficiency(self.attributes.get(Attribute::WaterMovementEfficiency)));
                    },
                    _ => {}
                }
            }
            bundle.write(&play::clientbound::UpdateAttributes {
                entity_id: self.entity_id,
                attribute: Cow::Owned(attribute_changes)
            });
        }

        // Synchronize abilities
        if self.abilities_dirty {
            self.abilities_dirty = false;
            
            bundle.write(&play::clientbound::PlayerAbilities {
                invulnerable: false,
                is_flying,
                allow_flying,
                instant_breaking: false,
                flying_speed: self.flying_speed,
                walking_speed: self.walking_speed,
            });

            known_state_changes.push(KnownClientStateChange::UpdateAbilities {
                is_flying,
                can_fly: allow_flying,
                flying_speed: self.flying_speed
            });
        }

        if !known_state_changes.is_empty() {
            let key = KnownClientStateChangeKey::StateSyncId(self.next_remote_state_sync_id);
            self.known_client_state_changes.push((self.keep_alive_timer, key, known_state_changes));

            bundle.write(&play::clientbound::Ping {
                id: 0x3B750000 | (self.next_remote_state_sync_id as i32 & 0xFFFF)
            });
            self.next_remote_state_sync_id = self.next_remote_state_sync_id.wrapping_add(1);
        }

        bundle.finish();

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

            let id = self.player_id.clone();
            self.chunk_ref = self.world_mut().put_player_into_chunk(id, chunk_x, chunk_z);

            let world = unsafe { self.world.as_mut() };

            self.send_packet(&play::clientbound::SetChunkCacheCenter {
                chunk_x,
                chunk_z,
            });

            // todo: use a thread local?
            let mut despawn_list = Vec::new();

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
                        world.write_spawn_entities_and_players(last_chunk_x+dx, last_chunk_z+dz, self);
                    } else {
                        world.write_despawn_entities_and_players(last_chunk_x+dx, last_chunk_z+dz, &mut despawn_list, self);
                    }
                }
            );

            if !despawn_list.is_empty() {
                let remove_entities = RemoveEntities {
                    entities: despawn_list.into(),
                };
                self.send_packet(&remove_entities);
            }
        }

        // Write position change packets
        self.write_update_position_packets();

        // Write viewable packets
        let include_self_bytes = self.include_self_packet_buffer.pop_written();
        let exclude_self_bytes = self.exclude_self_packet_buffer.pop_written();

        self.exclude_self_viewable_range = None;
        if !include_self_bytes.is_empty() || !exclude_self_bytes.is_empty() {
            if let Some(chunk) = unsafe { self.world.as_mut() }.get_chunk_mut(chunk_x, chunk_z) {
                chunk.entity_viewable.copy_bytes(include_self_bytes);

                if !exclude_self_bytes.is_empty() {
                    let start = chunk.entity_viewable.len();
                    chunk.entity_viewable.copy_bytes(exclude_self_bytes);
                    self.exclude_self_viewable_range = Some(start .. chunk.entity_viewable.len());
                }
            } else {
                self.include_self_packet_buffer.clear();
                self.exclude_self_packet_buffer.clear();
            }
        }

        self.last_position = self.position;
    }
    
    pub(super) fn acknowledge_pending_remote_state(&mut self, id: KnownClientStateChangeKey) -> bool {
        let Some(&(_, first_id, _)) = self.known_client_state_changes.first() else {
            return false;
        };
        if first_id != id {
            return false;
        }
        let (_, _, changes) = self.known_client_state_changes.remove(0);
        for change in changes {
            change.apply(self);
        }
        true
    }
    
    fn write_update_position_packets(&mut self) {
        // Write entity movement
        let new_rotation = (
            ByteRotation::from_f32(self.pitch),
            ByteRotation::from_f32(self.yaw)
        );
    
        let Some(synced_position) = &mut self.synced_position else {
            // Force teleport for first tick
            let teleport_packet = EntityPositionSync {
                entity_id: self.entity_id,
                x: self.position.x as _,
                y: self.position.y as _,
                z: self.position.z as _,
                x_vel: 0.0,
                y_vel: 0.0,
                z_vel: 0.0,
                yaw: self.yaw as _,
                pitch: self.pitch as _,
                on_ground: self.on_ground,
            };
            self.add_viewable_packet(&teleport_packet, PacketViewability::ExcludeSelf);

            let rotate_head_packet = RotateHead {
                entity_id: self.entity_id,
                head_yaw: self.yaw,
            };
            self.add_viewable_packet(&rotate_head_packet, PacketViewability::ExcludeSelf);
    
            self.old_rotation = new_rotation;
            self.position_sync_time = 0;
            self.synced_position = Some(self.position);
            return;
        };
    
        let delta = self.position - *synced_position;
        let quantized = delta * 4096.0;
    
        if self.position_sync_time < 400 {
            self.position_sync_time += 1;
        }

        if quantized.abs().max_element() < 1.0 {
            if self.old_rotation != new_rotation {
                let move_packet = MoveEntityRot {
                    entity_id: self.entity_id,
                    yaw: self.yaw,
                    pitch: self.pitch,
                    on_ground: self.on_ground,
                };
                self.add_viewable_packet(&move_packet, PacketViewability::ExcludeSelf);
    
                let rotate_head_packet = RotateHead {
                    entity_id: self.entity_id,
                    head_yaw: self.yaw,
                };
                self.add_viewable_packet(&rotate_head_packet, PacketViewability::ExcludeSelf);

                self.old_rotation = new_rotation;
            }
            return;
        }
    
        if quantized.min_element() <= i16::MIN as f64 || quantized.max_element() >= i16::MAX as f64 || self.position_sync_time >= 400 {
            // Force teleport due to large distance or 20 seconds since last teleport
            let teleport_packet = EntityPositionSync {
                entity_id: self.entity_id,
                x: self.position.x,
                y: self.position.y,
                z: self.position.z,
                x_vel: 0.0,
                y_vel: 0.0,
                z_vel: 0.0,
                yaw: self.yaw,
                pitch: self.pitch,
                on_ground: self.on_ground,
            };
            self.add_viewable_packet(&teleport_packet, PacketViewability::ExcludeSelf);
    
            let rotate_head_packet = RotateHead {
                entity_id: self.entity_id,
                head_yaw: self.yaw,
            };
            self.add_viewable_packet(&rotate_head_packet, PacketViewability::ExcludeSelf);

            self.old_rotation = new_rotation;
            self.position_sync_time = 0;
            self.synced_position = Some(self.position);
        } else {
            // Relative move
            let quantized = quantized.as_i16vec3();
            *synced_position += quantized.as_dvec3() / 4096.0;
    
            if self.old_rotation != new_rotation {
                let move_packet = MoveEntityPosRot {
                    entity_id: self.entity_id,
                    delta_x: quantized.x,
                    delta_y: quantized.y,
                    delta_z: quantized.z,
                    yaw: self.yaw as _,
                    pitch: self.pitch as _,
                    on_ground: self.on_ground,
                };
                self.add_viewable_packet(&move_packet, PacketViewability::ExcludeSelf);
    
                let rotate_head_packet = RotateHead {
                    entity_id: self.entity_id,
                    head_yaw: self.yaw,
                };
                self.add_viewable_packet(&rotate_head_packet, PacketViewability::ExcludeSelf);

                self.old_rotation = new_rotation;
            } else {
                let move_packet = MoveEntityPos {
                    entity_id: self.entity_id,
                    delta_x: quantized.x,
                    delta_y: quantized.y,
                    delta_z: quantized.z,
                    on_ground: false,
                };
                self.add_viewable_packet(&move_packet, PacketViewability::ExcludeSelf);
            }
        }
    }

    fn view_tick(&mut self) {
        let world = unsafe {
            self.world.as_mut()
        };

        let block_x = self.position.x.floor() as i32;
        let block_z = self.position.z.floor() as i32;
        let chunk_x = block_x >> 4;
        let chunk_z = block_z >> 4;

        let close_chunk_min_x = if block_x - chunk_x*16 < 8 {
            chunk_x - 1
        } else {
            chunk_x
        };let close_chunk_min_z = if block_z - chunk_z*16 < 8 {
            chunk_z - 1
        } else {
            chunk_z
        };

        // Chunk viewable
        let view_distance = P::World::VIEW_DISTANCE as i32;
        for x in (chunk_x-view_distance).max(0) .. (chunk_x+view_distance+1).min(world.chunks_x) {
            for z in (chunk_z-view_distance).max(0) .. (chunk_z+view_distance+1).min(world.chunks_z) {
                let chunk = world.get_chunk_mut(x, z).unwrap();

                if !chunk.single_block_changes.is_empty() {
                    if x >= close_chunk_min_x && x <= close_chunk_min_x+1 && z >= close_chunk_min_z && z <= close_chunk_min_z+1 {
                        let mut changed_positions = Vec::new();

                        let mut bundle = BundledPacketBuffer::new(&mut self.packet_buffer);

                        for (pos, (old, new)) in chunk.single_block_changes.iter() {
                            let unpacked_x = ((*pos >> 28) & 0xF) as i32 + x * 16;
                            let unpacked_y = ((*pos >> 4) & 0xFFFFFF) as i32;
                            let unpacked_z = (*pos & 0xF) as i32 + z * 16;
                            bundle.write(&play::clientbound::BlockUpdate {
                                pos: BlockPosition::new(unpacked_x, unpacked_y, unpacked_z),
                                block_state: *new as i32,
                            });

                            let position = IVec3::new(unpacked_x, unpacked_y, unpacked_z);
                            changed_positions.push(position);
                            self.known_client_state.known_blocks.insert(position, *old);
                        }

                        let mut known_state_changes = Vec::with_capacity(1);
                        known_state_changes.push(KnownClientStateChange::AcknowledgeBlockUpdates { positions: changed_positions });

                        let key = KnownClientStateChangeKey::StateSyncId(self.next_remote_state_sync_id);
                        self.known_client_state_changes.push((self.keep_alive_timer, key, known_state_changes));
            
                        bundle.write(&play::clientbound::Ping {
                            id: 0x3B750000 | (self.next_remote_state_sync_id as i32 & 0xFFFF)
                        });
                        self.next_remote_state_sync_id = self.next_remote_state_sync_id.wrapping_add(1);
                    } else {
                        for (pos, (_, new)) in chunk.single_block_changes.iter() {
                            let unpacked_x = ((*pos >> 28) & 0xF) as i32 + x * 16;
                            let unpacked_y = ((*pos >> 4) & 0xFFFFFF) as i32;
                            let unpacked_z = (*pos & 0xF) as i32 + z * 16;
                            play::clientbound::BlockUpdate {
                                pos: BlockPosition::new(unpacked_x, unpacked_y, unpacked_z),
                                block_state: *new as i32,
                            }.write_packet(&mut self.packet_buffer);
                        }
                    }
                }

                self.packet_buffer.copy_from(&chunk.chunk_viewable);
            }
        }

        // Entity viewable
        let self_chunk_x = (self.position.x / 16.0).floor() as i32;
        let self_chunk_z = (self.position.z / 16.0).floor() as i32;
        let view_distance = P::World::ENTITY_VIEW_DISTANCE as i32;
        for x in (chunk_x-view_distance).max(0) .. (chunk_x+view_distance+1).min(world.chunks_x) {
            for z in (chunk_z-view_distance).max(0) .. (chunk_z+view_distance+1).min(world.chunks_z) {
                let chunk = world.get_chunk_mut(x, z).unwrap();

                if !chunk.entity_viewable.is_empty() {
                    let entity_viewable = chunk.entity_viewable.peek_written();

                    if x == self_chunk_x && z == self_chunk_z {
                        if let Some(range) = self.exclude_self_viewable_range.take() {
                            let start = range.start;
                            let end = range.end;

                            if start > 0 {
                                self.packet_buffer.copy_bytes(&entity_viewable[..start]);
                            }
                            if end < entity_viewable.len() {
                                self.packet_buffer.copy_bytes(&entity_viewable[end..]);
                            }

                            continue;
                        }
                    }

                    self.packet_buffer.copy_bytes(entity_viewable);
                }
            }
        }
        self.exclude_self_viewable_range = None;

        // Send block change ack
        if let Some(ack_sequence_up_to) = self.ack_sequence_up_to {
            self.send_packet(&BlockChangedAck { sequence: ack_sequence_up_to });
            self.ack_sequence_up_to = None;
        }

        self.flush_packets();
    }

    pub(crate) fn try_abort_using_item(&mut self) {
        if self.using_item.is_some() {
            P::abort_using_item(self);
            self.metadata.set_living_entity_flags(self.metadata.living_entity_flags & !1);
            self.using_item = None;
        } else if (self.metadata.living_entity_flags & 1) != 0 {
            self.metadata.set_living_entity_flags(self.metadata.living_entity_flags & !1);
        }
    }

    pub(crate) fn try_begin_using_item(&mut self, hand: InteractionHand, on_block: Option<BlockHitResult>) {
        if self.packets_handled_this_tick.contains(PacketHandledThisTick::SuccessfulInteractOrUseItem) {
            return;
        }

        let hand_specific_use = match hand {
            InteractionHand::MainHand => PacketHandledThisTick::UseItemMainHand,
            InteractionHand::OffHand => PacketHandledThisTick::UseItemOffHand,
        };
        if !self.packets_handled_this_tick.insert(hand_specific_use) {
            return;
        }

        let hotbar_slot = match hand {
            InteractionHand::MainHand => InventorySlot::Hotbar(self.hotbar_slot),
            InteractionHand::OffHand => InventorySlot::OffHand,
        };
        let item_stack = self.container_view.get_inventory_item_stack(hotbar_slot);

        if let Some(using_item) = &self.using_item {
            if using_item.hand != hand {
                return;
            }
            if using_item.slot == hotbar_slot && using_item.item_stack.item == item_stack.item {
               return;
            } else {
                self.try_abort_using_item();
            }
        }

        if item_stack.is_empty() {
            return;
        }

        if let Some(equippable) = item_stack.components.get::<Equippable>() {
            if equippable.inner.swappable {
                let swap_target = match equippable.inner.slot {
                    EquipmentSlot::Mainhand => InventorySlot::Hotbar(self.hotbar_slot),
                    EquipmentSlot::Offhand => InventorySlot::OffHand,
                    EquipmentSlot::Feet => InventorySlot::Feet,
                    EquipmentSlot::Legs => InventorySlot::Legs,
                    EquipmentSlot::Chest => InventorySlot::Chest,
                    EquipmentSlot::Head => InventorySlot::Head,
                    _ => hotbar_slot,
                };
                if swap_target != hotbar_slot {
                    let action = self.container_view.do_container_action(ContainerAction::Swap(hotbar_slot, swap_target));
                    if let Some(action) = action {
                        (action)(self);
                    }
                    self.packets_handled_this_tick.insert(PacketHandledThisTick::SuccessfulInteractOrUseItem);
                }
                self.container_view.force_synchronize_slot(swap_target);
                return;
            }
        }

        match P::use_item(self, hand, on_block) {
            UseItemResult::Nothing => {
                return;
            },
            UseItemResult::UsedItem => {
                self.packets_handled_this_tick.insert(PacketHandledThisTick::SuccessfulInteractOrUseItem);
            },
            UseItemResult::StartUsingItem => {
                self.packets_handled_this_tick.insert(PacketHandledThisTick::SuccessfulInteractOrUseItem);
                self.metadata.set_living_entity_flags(self.metadata.living_entity_flags | 1);
                self.using_item = Some(UsingItem {
                    item_stack,
                    hand,
                    slot: hotbar_slot,
                    ticks: 0
                });
            },
        }
    }

    pub fn reset_fall_distance(&mut self) {
        self.fall_distance = 0.0;
    }

    pub(crate) fn set_on_ground(&mut self, on_ground: bool) {
        if self.fall_distance > 0.01 && on_ground && !self.on_ground {
            P::land_after_falling(self, self.fall_distance);
            self.fall_distance = 0.0;
        }
        self.on_ground = on_ground;
    }
}

#[derive(Debug)]
pub(crate) struct UsingItem {
    pub(crate) item_stack: ItemStack,
    pub(crate) hand: InteractionHand,
    pub(crate) slot: InventorySlot,
    pub(crate) ticks: usize
}

#[derive(Default)]
pub struct FluidHeights {
    pub water: Option<f64>,
    pub lava: Option<f64>,
}

// Some packets should only be sent by the client once per click tick
// Enforcing this on the server reduces the effectiveness of cheats/exploits 
#[derive(EnumSetType)]
pub enum PacketHandledThisTick {
    Movement,
    SwitchSlot,
    PlayerInput,
    Sneak,
    Sprint,
    AttackEntity,
    SuccessfulInteractOrUseItem,
    InteractWithEntityMainHand,
    InteractWithEntityOffHand,
    PlaceRecipe,
    UseItemMainHand,
    UseItemOffHand,
}