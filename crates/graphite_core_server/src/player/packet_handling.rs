use std::{borrow::Cow, cell::RefCell, collections::{HashMap, HashSet}, ops::ControlFlow, time::{Duration, Instant}};

use anyhow::bail;
use enum_map::EnumMap;
use enumset::EnumSet;
use float_next_after::NextAfter;
use glam::{DVec2, DVec3, IVec3, Vec2, Vec3, Vec3Swizzles};
use graphite_binary::slice_serialization::{slice_serializable, BigEndian, SliceSerializable};
use graphite_command::types::{ParseErrorType, Span};
use graphite_mc_constants::{block::{Block, BlockAttributes, BlockClass, BlockFlag, BlockState}, builtin::{Attribute, ContainerType, MobEffect}, fluid::{Fluid, FluidAttributes, FluidState}, item::Item, types::{EntityAnimation, HandAction, HumanoidArm, InteractionHand, MoveAction, Pose, RelativeMovement}};
use graphite_mc_protocol::{play::{self, clientbound::CommandSuggestionEntry, serverbound::{AcceptTeleportation, KeepAlive, MovePlayerPos, MovePlayerPosRot, MovePlayerRot, PacketHandler, PlayerHandAction, SetCarriedItem, UseItemOn}}, types::{data_component::Consumable, text::{IntoTextComponent, TextComponent}}, IdentifiedPacket};
use graphite_network::{FramedPacketHandler, HandleAction};
use rustc_hash::FxHashMap;

use crate::{entity::EntityBase, inventory::{container::Container, inventory_slot::InventorySlot, menu::ContainerAction}, types::AABB, world::{chunk::ChunkProvider, BlockGetter, World, WorldExtension}};

use super::{known_client_state::{DebugState, ForcedPose, KnownClientStateChangeKey, UncertainVelocity}, language::Language, mojang_math, DropCause, FluidHeights, GenericPlayer, PacketHandledThisTick, PacketViewability, Player, PlayerExtension, SwingCause};

enum EntityInteractionResult {
    UnknownEntity,
    Failed,
    Success
}

impl <P: PlayerExtension> Player<P> {
    fn handle_pre_chat_and_check_allowed(&mut self) -> bool {
        // Close any containers since it's impossible to send messages with one open
        self.close_menu();

        // Only allow chat packet if no container is opened
        self.container_view.get_container_type().is_none()
    }

    fn update_rotation_from_client(&mut self, yaw: f32, pitch: f32) -> anyhow::Result<()> {
        if is_invalid_rotation(yaw, pitch) {
            bail!("invalid move value");
        }

        self.yaw = yaw;
        self.pitch = pitch;

        Ok(())
    }

    fn update_position_from_client(&mut self, client_position: DVec3) -> anyhow::Result<()> {
        if is_invalid_movement(client_position.x, client_position.y, client_position.z) {
            bail!("invalid move value");
        }

        self.packets_handled_this_tick |= PacketHandledThisTick::Movement;

        // updatingUsingItem
        if (self.known_client_state.living_flags & 1) != 0 {
            if self.known_client_state.held_item_stack.item != self.known_client_state.use_item {
                self.known_client_state.living_flags &= !1;
                self.known_client_state.use_item = Item::Air;
                self.known_client_state.velocity.ignored_item_slow = false;
            }
        } else {
            self.known_client_state.velocity.ignored_item_slow = false;
        }

        if self.known_client_state.velocity.ignored_item_slow {
            // The player ignored the item slow last tick, but didn't cancel the using item this tick
            // We lag them back and cancel this movement in order to punish the player, preventing cheats from just ignoring the slow
            // This is necessary since held item changes aren't necessarily sent until the next tick
            if crate::debug::DEBUG_MOVEMENT {
                self.send_chat_message("Lagged back due to anti-noslow!".into_text_component());
            }
            
            self.known_client_state.velocity *= DVec3::new(0.2, 1.0, 0.2);
            self.teleport_position_and_velocity(self.position, self.known_client_state.velocity.get_expected());

            self.last_client_position = self.position;
            self.known_client_state.velocity.ignored_item_slow = false;
            return Ok(());
        }

        if self.known_client_state.awaiting_absolute_teleport_count > 0 || self.camera_entity_id != self.entity_id || self.is_sleeping()  {
            self.no_jump_delay = u8::MAX;
            self.fluid_at_eyes = get_fluid_at_eye(self);
            return Ok(());
        }

        let check_aabb = self.create_collision_aabb_at(self.last_client_position);
        let check_aabb = check_aabb.expand(self.known_client_state.velocity.get_expected());
        let check_aabb = check_aabb.inflate(1.0).unwrap();
        let (solid_entities, soft_entities) = count_collidable_entities(self.world(), check_aabb, self.get_uuid());

        // Take into account solid/soft entities from previous ticks to account for lag
        let mut trust_client = solid_entities > 0;
        let mut total_collidable_entities: f64 = solid_entities as f64 + soft_entities as f64;
        for (last_solid_count, last_soft_count) in self.last_entity_collision_counts.iter().copied() {
            if last_solid_count > 0 {
                trust_client = true;
            }
            total_collidable_entities = total_collidable_entities.max(last_solid_count as f64 + last_soft_count as f64);
        }

        // Add current solid/soft count to the deque
        if self.last_entity_collision_counts.len() >= 8 {
            self.last_entity_collision_counts.pop_back();
        }
        self.last_entity_collision_counts.push_front((solid_entities, soft_entities));

        // slightly more than 0.05 because of rounding errors
        let collision_uncertainty = 0.05000001 * total_collidable_entities;
        self.known_client_state.velocity.add_horizontal_uncertainty(collision_uncertainty);

        // Sprint fix - client won't send sprinting if it hasn't changed since last tick.
        // This can be problematic if the server sends a shared_flag metadata change to the client
        // and then immediately starts sprinting again. Even though the client started sprinting,
        // the server is not informed of this. Here we detect a mismatch and correct it.
        let is_sprinting_on_server = (self.known_client_state.shared_flags & (1 << 3)) != 0;
        if self.is_sprinting() != is_sprinting_on_server {
            if self.is_sprinting() {
                self.set_sprinting_from_client(true);
            } else {
                self.set_sprinting_from_client(false);
            }
        }

        let prediction = predict_movement(self, client_position, trust_client);

        if crate::debug::DEBUG_MOVEMENT && prediction.on_ground != self.last_client_on_ground {
            let msg = format!("Misprediction for onGround: server: {}, client: {}", prediction.on_ground, self.last_client_on_ground);
            self.send_chat_message(msg.into_text_component());
        }
        if crate::debug::DEBUG_MOVEMENT && prediction.horizontal_collision != self.last_client_horizontal_collision {
            let msg = format!("Misprediction for horizontalCollision: server: {}, client: {}", prediction.horizontal_collision, self.last_client_horizontal_collision);
            self.send_chat_message(msg.into_text_component());
        }

        // Be lenient and move our predicted position towards the client position by 0.0001
        let predicted_position = self.position + prediction.movement;
        let adjusted_position = move_towards(predicted_position, client_position, 1E-4);

        let previous_position = self.position;

        self.position = predicted_position;
        if self.position != adjusted_position {
            let adjustment = vanilla_shape_cast(KnownBlockGetter::new(self), self.create_collision_aabb(), adjusted_position - self.position);
            self.position += adjustment;
        }

        // Update fall distance
        let new_fall_distance = (self.fall_distance - (self.position.y - previous_position.y) as f32).max(0.0);
        if prediction.reset_fall_distance || prediction.fluid_heights.water.is_some() || self.is_flying() || new_fall_distance == 0.0 {
            self.fall_distance = 0.0;
        } else if previous_position != self.position {
            let reset_fall_distance = self.world().raycast(previous_position, self.position, |_, _, _, block_state| {
                if block_state != 0 {
                    let attr = BlockAttributes::from_block_state(block_state);
                    if attr.has_flag(BlockFlag::FallDamageResetting) {
                        return ControlFlow::Break(true);
                    }
                }
                ControlFlow::Continue(())
            });
            if reset_fall_distance == Some(true) {
                self.fall_distance = 0.0;
            } else {
                self.fall_distance = new_fall_distance;
                if prediction.fluid_heights.lava.is_some() {
                    self.fall_distance *= 0.5;
                }
            }
        }

        // Update state based on prediction
        self.set_on_ground(prediction.on_ground); // Will also handle fall damage
        self.no_jump_delay = prediction.no_jump_delay;
        self.stuck_multiplier = prediction.stuck_multiplier;
        self.known_client_state.velocity = prediction.velocity;
        self.main_supporting_block = prediction.main_supporting_block;
        self.fluid_heights = prediction.fluid_heights;

        self.set_swimming(prediction.is_swimming);
        
        // The client will predict swimming, so we need to update the known state
        if prediction.is_swimming {
            self.known_client_state.shared_flags |= 1 << 4;
        } else {
            self.known_client_state.shared_flags &= !(1 << 4);
        }

        // Client's fluidOnEyes is always 1 tick out of date, update it before changing the position
        self.fluid_at_eyes = get_fluid_at_eye(self);

        if self.position.distance_squared(client_position) > 0.01*0.01 {
            // Lag back
            if crate::debug::DEBUG_MOVEMENT || true {
                self.send_chat_message("Lagged back!".into_text_component());
                println!("Lagged back!!");
                println!("Result: {:?}", self.position);
                println!("Predicted: {:?}", predicted_position);
                println!("Client: {:?}", client_position);
                println!("Predicted delta: {:?}", prediction.movement);
                println!("Client delta: {:?}", client_position - self.last_client_position);
            }

            self.teleport_position_and_velocity(self.position, self.known_client_state.velocity.get_expected());
            self.last_client_position = self.position;
            self.known_client_state.velocity.ignored_item_slow = false;
        } else {
            self.last_client_position = client_position;
        }

        Ok(())
    }
    
    fn try_attack_entity(&mut self, entity_id: i32) -> EntityInteractionResult {
        let eye = self.eye_position();
        let reach = self.attributes.get(Attribute::EntityInteractionRange);
        let world = self.world();

        let Some(&remote_entity_id) = world.remote_entity_by_network_id.get(&entity_id) else {
            return EntityInteractionResult::UnknownEntity;
        };

        let entity = &world.remote_entities[remote_entity_id];
        let entity_id = entity.ecs_id;

        if !entity.is_attackable() {
            return EntityInteractionResult::Failed;
        }
        let Some(aabb) = entity.create_aabb() else {
            return EntityInteractionResult::Failed;
        };

        let distance = aabb.distance_to_point(eye);
        if distance > reach + P::REACH_THRESHOLD {
            return EntityInteractionResult::Failed;
        }

        // Check for velocity reduction
        let is_sprinting = (self.known_client_state.shared_flags & (1 << 3)) != 0;
        let has_attack_damage = if self.known_client_state.living_flags & (1 << 2) != 0 {
            true // auto spin attack always deals damage
        } else {
            self.attributes.get(Attribute::AttackDamage) > 0.0 // we should use known_client_state but it's probably fine
        };
        if !entity.skip_attack_interaction() && entity.can_hurt_client() && is_sprinting && has_attack_damage {
            self.known_client_state.velocity.maybe_multiply(0.6, 0.6);
        }

        if self.attacked_entity {
            // In addition to only allowing the attack every client tick, we also only allow it every server tick
            return EntityInteractionResult::Failed;
        }

        if let Some(base) = self.world().query_one::<&EntityBase>(entity_id).get() {
            if !base.is_valid() {
                return EntityInteractionResult::Failed;
            }
        } else {
            return EntityInteractionResult::Failed;
        }

        self.attacked_entity = true;
        P::attack_entity(self, entity_id, distance.min(reach));
        return EntityInteractionResult::Success
    }
    
    fn try_interact_with_entity_at(&mut self, id: i32, offset_x: f32, offset_y: f32, offset_z: f32, hand: InteractionHand) -> EntityInteractionResult {
        let eye = self.eye_position();
        let reach = self.attributes.get(Attribute::EntityInteractionRange);
        let world = self.world();
    
        let Some(&remote_entity_id) = world.remote_entity_by_network_id.get(&id) else {
            return EntityInteractionResult::UnknownEntity;
        };

        let entity = &world.remote_entities[remote_entity_id];

        if let Some(base) = world.query_one::<&EntityBase>(entity.ecs_id).get() {
            if !base.is_valid() {
                return EntityInteractionResult::Failed;
            }
        } else {
            return EntityInteractionResult::Failed;
        }

        let Some(aabb) = entity.create_aabb() else {
            return EntityInteractionResult::Failed;
        };

        let distance = aabb.distance_to_point(eye);
        if distance < reach + P::REACH_THRESHOLD {
            let entity = entity.ecs_id;
            let offset = Vec3::new(offset_x, offset_y, offset_z);
            if P::interact_entity_at(self, entity, hand, offset, distance.min(reach)) {
                return EntityInteractionResult::Success;
            }
        }
        return EntityInteractionResult::Failed;
    }
}

impl <P: PlayerExtension> FramedPacketHandler for Player<P> {
    fn handle(&mut self, data: &[u8]) -> HandleAction {
        if !self.is_still_connected() {
            return HandleAction::Continue;
        }

        let parse = self.parse_and_handle(data, |player, packet| {
            packet == play::serverbound::PacketId::SetCreativeModeSlot
        });
        match parse {
            Ok(()) => HandleAction::Continue,
            Err(error) => {
                // Disconnect player with error message
                let error_string = format!("Error: {}", error);
                println!("Disconnecting {} ({:x}) for \"{}\"", self.profile.username, self.profile.uuid, error_string);
                play::clientbound::Disconnect {
                    message: TextComponent::literal_owned(error_string).to_encoded_nbt(),
                }.write_packet(&mut self.packet_buffer);
                self.flush_packets();

                HandleAction::Disconnect
            },
        }
    }

    fn disconnected(&mut self) {
        if self.connection.is_some() {
            P::before_disconnect(self);
            self.connection = None;
        }
    }
}

impl <P: PlayerExtension> graphite_mc_protocol::play::serverbound::PacketHandler for Player<P> {
    const DEBUG: bool = false;

    fn handle_bundle_item_selected(&mut self, bundle_item_selected: play::serverbound::BundleItemSelected) -> anyhow::Result<()> {
        let function = self.container_view.do_bundle_item_selected(bundle_item_selected)?;
        if let Some(function) = function {
            (function)(self);
        }
        Ok(())
    }

    fn handle_chat_command(&mut self, command: play::serverbound::ChatCommand) -> anyhow::Result<()> {
        if command.command.is_empty() {
            return Ok(());
        }

        if !self.handle_pre_chat_and_check_allowed() {
            return Ok(());
        }

        let reborrowed_self = unsafe { (self as *mut Self).as_mut()}.unwrap();
        if let Some(commands) = &self.commands {
            let result = commands.dispatch(reborrowed_self, command.command);

            let mut component;
            let mut context_span = Span {
                start: 0,
                end: command.command.len()-1,
            };

            match result {
                graphite_command::types::CommandDispatchResult::Success(result) => {
                    if let Err(error) = result {
                        self.send_chat_message(TextComponent::literal(&error).red());
                    }
                    return Ok(())
                },
                graphite_command::types::CommandDispatchResult::ParseError {
                    span,
                    error,
                    continue_parsing: _
                } => {
                    match error {
                        ParseErrorType::FloatTooBig { max, found } => {
                            component = TextComponent::translatable2("argument.float.big", max, found);
                        },
                        ParseErrorType::FloatTooSmall { min, found } => {
                            component = TextComponent::translatable2("argument.float.low", min, found);
                        },
                        ParseErrorType::ExpectedFloat => {
                            component = TextComponent::translatable0("parsing.float.expected");
                        },
                        ParseErrorType::IntegerTooBig { max, found } => {
                            component = TextComponent::translatable2("argument.integer.big", max, found);
                        },
                        ParseErrorType::IntegerTooSmall { min, found } => {
                            component = TextComponent::translatable2("argument.integer.low", min, found);
                        },
                        ParseErrorType::ExpectedInteger => {
                            component = TextComponent::translatable0("parsing.int.expected");
                        },
                        ParseErrorType::ExpectedBoolean => {
                            component = TextComponent::translatable0("parsing.bool.expected");
                        }
                        ParseErrorType::Other(other) => {
                            component = TextComponent::translatable1("argument.enum.invalid", other);
                        },
                    }
                    context_span = span;
                },
                graphite_command::types::CommandDispatchResult::UnknownCommand { span } => {
                    component = TextComponent::translatable0("command.unknown.command");
                    context_span = span;
                },
                graphite_command::types::CommandDispatchResult::IncompleteCommand => {
                    component = TextComponent::translatable0("command.unknown.command");
                },
                graphite_command::types::CommandDispatchResult::TooManyArguments { span } => {
                    component = TextComponent::translatable0("command.unknown.argument");
                    context_span = span;
                },
            }

            component = component.red().append("\n");

            if context_span.start > 0 {
                component = component.append(command.command[0..context_span.start].into_text_component().gray());
            };

            component = component.append(command.command[context_span.start..=context_span.end].into_text_component());
            component = component.append(TextComponent::translatable0("command.context.here"));

            self.send_chat_message(component);
        }

        Ok(())
    }

    fn handle_client_action(&mut self, client_action: play::serverbound::ClientAction) -> anyhow::Result<()> {
        P::client_action(self, client_action.action);
        Ok(())
    }

    fn handle_player_move_action(&mut self, move_action: play::serverbound::PlayerMoveAction) -> anyhow::Result<()> {
        match move_action.action {
            MoveAction::PressShiftKey => {
                if !self.packets_handled_this_tick.insert(PacketHandledThisTick::Sneak) {
                    bail!("duplicate packet: sneak")
                }
                self.set_sneaking(true);
                self.known_client_state.shared_flags |= 1 << 1;
            },
            MoveAction::ReleaseShiftKey => {
                if !self.packets_handled_this_tick.insert(PacketHandledThisTick::Sneak) {
                    bail!("duplicate packet: sneak")
                }
                self.set_sneaking(false);
                self.known_client_state.shared_flags &= !(1 << 1);
            },
            MoveAction::StopSleeping => self.set_sleeping_pos(None),
            MoveAction::StartSprinting => {
                if !self.packets_handled_this_tick.insert(PacketHandledThisTick::Sprint) {
                    bail!("duplicate packet: sprint")
                }
                self.set_sprinting_from_client(true);
            },
            MoveAction::StopSprinting => {
                if !self.packets_handled_this_tick.insert(PacketHandledThisTick::Sprint) {
                    bail!("duplicate packet: sprint")
                }
                self.set_sprinting_from_client(false);
            },
            MoveAction::StartRidingJump => {},
            MoveAction::StopRidingJump => {},
            MoveAction::OpenInventory => {},
            MoveAction::StartFallFlying => {},
        }
        Ok(())
    }

    fn handle_player_input(&mut self, player_input: play::serverbound::PlayerInput) -> anyhow::Result<()> {
        self.player_loaded_time = 0;

        if !self.packets_handled_this_tick.insert(PacketHandledThisTick::PlayerInput) {
            bail!("duplicate packet: player input")
        }

        if player_input.forward == player_input.backward {
            self.forwards_input = 0;
        } else if player_input.forward {
            self.forwards_input = 1;
        } else if player_input.backward {
            self.forwards_input = -1;
        }
        if player_input.right == player_input.left {
            self.strafe_input = 0;
        } else if player_input.right {
            self.strafe_input = 1;
        } else if player_input.left {
            self.strafe_input = -1;
        }
        self.shift_pressed = player_input.shift;
        self.jump_pressed = player_input.jump;
        Ok(())
    }

    fn handle_player_loaded(&mut self, _: play::serverbound::PlayerLoaded) -> anyhow::Result<()> {
        self.player_loaded_time = 0;
        Ok(())
    }

    fn handle_pong(&mut self, pong: play::serverbound::Pong) -> anyhow::Result<()> {
        if ((pong.id >> 16) & 0xFFFF) == 0x3B75 {
            let remote_sync_id = (pong.id & 0xFFFF) as u16;
            if self.acknowledge_pending_remote_state(KnownClientStateChangeKey::StateSyncId(remote_sync_id)) {
                return Ok(());
            }
        }
        P::handle_ping_pong_response(self, pong.id);
        Ok(())
    }

    fn handle_accept_teleportation(&mut self, accept_teleportation: AcceptTeleportation) -> anyhow::Result<()> {
        if self.acknowledge_pending_remote_state(KnownClientStateChangeKey::TeleportId(accept_teleportation.id)) {
            return Ok(());
        }

        Ok(())
    }

    fn handle_place_recipe(&mut self, recipe: play::serverbound::PlaceRecipe) -> anyhow::Result<()> {
        if self.container_view.get_container_id() != recipe.container_id {
            return Ok(());
        }
        if !self.packets_handled_this_tick.insert(PacketHandledThisTick::PlaceRecipe) {
            return Ok(());
        }
        match self.container_view.get_container_type() {
            Some(ContainerType::BlastFurnace) => {},
            Some(ContainerType::Furnace) => {},
            Some(ContainerType::Smoker) => {},
            Some(ContainerType::Crafting) => {},
            None => {},
            _ => {
                return Ok(());
            }
        }

        P::handle_place_recipe(self, recipe);
        Ok(())
    }

    fn handle_player_abilities(&mut self, player_abilities: play::serverbound::PlayerAbilities) -> anyhow::Result<()> {
        let is_flying = (player_abilities.flags & 2) != 0;

        self.known_client_state.is_flying = is_flying && self.known_client_state.can_fly;
        self.set_flying_raw(is_flying && self.can_fly());
        
        Ok(())
    }

    fn handle_player_hand_action(&mut self, player_hand_action: PlayerHandAction) -> anyhow::Result<()> {
        self.ack_sequence_up_to = Some(player_hand_action.sequence);

        match player_hand_action.action {
            HandAction::StartDestroyBlock => {   
                self.try_abort_using_item();
                if !self.allow_world_interaction() {
                    return Ok(());
                }
                // Start destroy block logic goes here
                Ok(())
            },
            HandAction::AbortDestroyBlock => {
                self.try_abort_using_item();
                if !self.allow_world_interaction() {
                    return Ok(());
                }
                // Abort destroy block logic goes here
                Ok(())
            },
            HandAction::StopDestroyBlock => {
                self.try_abort_using_item();
                if !self.allow_world_interaction() {
                    return Ok(());
                }
                // Stop destroy block logic goes here
                Ok(())
            },
            HandAction::DropAllItems => {
                if self.allow_world_interaction() {
                    P::drop_item_in_hand(self, true);
                }

                let hotbar_slot = InventorySlot::Hotbar(self.hotbar_slot);
                self.container_view.force_synchronize_slot(hotbar_slot);
                Ok(())
            },
            HandAction::DropItem => {
                self.last_swing_cause = SwingCause::Drop;

                if self.allow_world_interaction() {
                    P::drop_item_in_hand(self, false);
                }

                let hotbar_slot = InventorySlot::Hotbar(self.hotbar_slot);
                self.container_view.force_synchronize_slot(hotbar_slot);
                Ok(())
            },
            HandAction::ReleaseUseItem => {
                self.known_client_state.living_flags &= !1;

                if !self.allow_world_interaction() {
                    self.try_abort_using_item();
                    return Ok(());
                }

                if let Some(using_item) = &self.using_item {
                    let using_slot = match using_item.hand {
                        InteractionHand::MainHand => InventorySlot::Hotbar(self.hotbar_slot),
                        InteractionHand::OffHand => InventorySlot::OffHand,
                    };
                    let active_item = self.container_view.get_inventory_item_stack(using_slot).item;

                    if using_item.slot == using_slot && using_item.item_stack.item == active_item {
                        P::finish_using_item(self, using_item.slot, using_item.ticks);
                        self.using_item = None;
                        self.metadata.set_living_entity_flags(self.metadata.living_entity_flags & !1);
                    } else {
                        self.try_abort_using_item();
                    }

                    self.container_view.force_synchronize_slot(using_slot);
                }
                
                self.container_view.force_synchronize_slot(InventorySlot::Hotbar(self.hotbar_slot));
                Ok(())
            },
            HandAction::SwapItemWithOffhand => {
                if self.allow_world_interaction() {
                    P::swap_item_with_off_hand(self);
                }
                Ok(())
            },
        }
    }

    fn handle_use_item_on(&mut self, use_item_on: UseItemOn) -> anyhow::Result<()> {
        self.ack_sequence_up_to = Some(use_item_on.sequence);
        let hotbar_slot = InventorySlot::Hotbar(self.hotbar_slot);
        self.container_view.force_synchronize_slot(hotbar_slot);

        self.last_swing_cause = SwingCause::InteractWithBlock;

        if !self.allow_world_interaction() {
            return Ok(());
        }

        self.try_begin_using_item(use_item_on.hand, Some(use_item_on.block_hit));
        Ok(())
    }

    fn handle_use_item(&mut self, use_item: play::serverbound::UseItem) -> anyhow::Result<()> {
        // Predict item usage for anticheat
        // note: missing handling for cooldowns
        if let Some(consumable) = self.known_client_state.held_item_stack.components.get::<Consumable>() {
            // note: missing check for canConsume if food
            let consume_ticks = (consumable.inner.consume_seconds * 20.0) as i32;
            if consume_ticks > 0 {
                // Predict using item
                self.known_client_state.living_flags |= 1;
                self.known_client_state.use_item = self.known_client_state.held_item_stack.item;
            }
        }

        // Handle use item
        self.yaw = use_item.yaw;
        self.pitch = use_item.pitch;
        self.ack_sequence_up_to = Some(use_item.sequence);
        let hotbar_slot = InventorySlot::Hotbar(self.hotbar_slot);
        self.container_view.force_synchronize_slot(hotbar_slot);

        self.last_swing_cause = SwingCause::UseItem;

        if !self.allow_world_interaction() {
            return Ok(());
        }

        self.try_begin_using_item(use_item.hand, None);
        Ok(())
    }

    fn handle_swing(&mut self, swing: play::serverbound::Swing) -> anyhow::Result<()> {
        if self.attack_strength_ticker > 0 {
            self.attack_strength_ticker = 0;
            P::attack_strength_reset(self);
        }
        self.pending_swing_hand = Some(swing.hand);
        Ok(())
    }

    fn handle_interact_entity(&mut self, interact_entity: play::serverbound::InteractEntity) -> anyhow::Result<()> {
        self.last_swing_cause = SwingCause::InteractWithEntity;

        if self.entity_id == interact_entity.entity_id {
            return Ok(());
        }

        if !self.allow_world_interaction() {
            return Ok(());
        }

        match interact_entity.mode {
            play::serverbound::InteractMode::Interact { hand: _ } => {},
            play::serverbound::InteractMode::Attack {  } => {
                if !self.packets_handled_this_tick.insert(PacketHandledThisTick::AttackEntity) {
                    return Ok(());
                }

                self.try_abort_using_item();

                match self.try_attack_entity(interact_entity.entity_id) {
                    EntityInteractionResult::UnknownEntity => {
                        P::attack_unknown_entity(self, interact_entity.entity_id);
                    },
                    _ => {},
                }

                if self.attack_strength_ticker > 0 {
                    self.attack_strength_ticker = 0;
                    P::attack_strength_reset(self);
                }
            },
            play::serverbound::InteractMode::InteractAt { offset_x, offset_y, offset_z, hand } => {
                if self.packets_handled_this_tick.contains(PacketHandledThisTick::SuccessfulInteractOrUseItem) {
                    return Ok(());
                }

                let hand_specific_use = match hand {
                    InteractionHand::MainHand => PacketHandledThisTick::InteractWithEntityMainHand,
                    InteractionHand::OffHand => PacketHandledThisTick::InteractWithEntityOffHand,
                };
                if !self.packets_handled_this_tick.insert(hand_specific_use) {
                    return Ok(());
                }

                self.try_abort_using_item();

                match self.try_interact_with_entity_at(interact_entity.entity_id, offset_x, offset_y, offset_z, hand) {
                    EntityInteractionResult::UnknownEntity => {
                        if P::interact_with_unknown_entity(self, interact_entity.entity_id, hand) {
                            self.packets_handled_this_tick.insert(PacketHandledThisTick::SuccessfulInteractOrUseItem);
                            return Ok(());
                        }
                    },
                    EntityInteractionResult::Failed => {},
                    EntityInteractionResult::Success => {
                        self.packets_handled_this_tick.insert(PacketHandledThisTick::SuccessfulInteractOrUseItem);
                        return Ok(());
                    },
                }

                self.try_begin_using_item(hand, None);
            },
        }

        Ok(())
    }

    fn handle_set_carried_item(&mut self, set_carried_item: SetCarriedItem) -> anyhow::Result<()> {
        if set_carried_item.slot > 8 {
            bail!("invalid slot")
        }

        let slot = set_carried_item.slot as u8;
        if self.hotbar_slot != slot {
            if !self.packets_handled_this_tick.insert(PacketHandledThisTick::SwitchSlot) {
                bail!("duplicate packet: set carried item")
            }

            if let Some(using_item) = &self.using_item {
                if using_item.hand == InteractionHand::MainHand {
                    self.try_abort_using_item();
                }
            }

            let hotbar_slot = InventorySlot::Hotbar(slot);
            let held_item_stack = self.container_view.get_inventory_item_stack(hotbar_slot);
            if held_item_stack.item != self.held_item_type {
                self.held_item_type = held_item_stack.item;
                if self.attack_strength_ticker > 0 {
                    self.attack_strength_ticker = 0;
                    P::attack_strength_reset(self);
                }
            }

            self.hotbar_slot = set_carried_item.slot as u8;
            self.known_client_state.held_item_stack = held_item_stack;
        }

        Ok(())
    }

    fn handle_keep_alive(&mut self, keep_alive: KeepAlive) -> anyhow::Result<()> {
        if self.current_keep_alive == keep_alive.id {
            self.current_keep_alive = 0;
        }
        Ok(())
    }

    fn handle_container_close(&mut self, container_close: play::serverbound::ContainerClose) -> anyhow::Result<()> {
        if let Some(held) = self.container_view.drop_held_item() {
            P::drop_item(self, held, DropCause::ContainerClosed);
        }

        let function = self.container_view.do_container_close(container_close);
        if let Some(function) = function {
            (function)(self);
        }
        self.container_view.force_synchronize_all();
        Ok(())
    }

    fn handle_custom_payload(&mut self, payload: play::serverbound::CustomPayload) -> anyhow::Result<()> {
        if payload.channel == "devutils:debug_velocities" {
            let mut bytes = payload.data;
            self.known_client_state.debug_state = DebugState::read_fully(&mut bytes)?;
        }
        
        Ok(())
    }

    fn handle_container_click(&mut self, container_click: play::serverbound::ContainerClick) -> anyhow::Result<()> {
        self.try_abort_using_item();

        if let Some(action) = self.container_view.get_container_action_from_click(container_click)? {
            if let ContainerAction::Throw { slot: _, all: _, outside: _ } = action {
                self.last_swing_cause = SwingCause::Drop;
            }
            if self.container_view.get_menu().is_none() {
                if P::on_inventory_clicked(self, action.clone()) {
                    return Ok(());
                }
            }

            let function = self.container_view.do_container_action(action);
            if let Some(function) = function {
                (function)(self);
            }
        }
        Ok(())
    }

    fn handle_command_suggestion(&mut self, command_suggestion: play::serverbound::CommandSuggestion) -> anyhow::Result<()> {
        if command_suggestion.command.is_empty() {
            return Ok(());
        }

        let mut offset = 0;
        if command_suggestion.command.starts_with("/") {
            offset = 1;
        }

        if !self.handle_pre_chat_and_check_allowed() {
            return Ok(());
        }

        if let Some(commands) = &self.commands {
            let result = commands.suggest(&command_suggestion.command[offset..]);

            if !result.values.is_empty() {
                let mut entries = Vec::new();

                let mut max_length = 0;
                for text in result.values {
                    max_length = max_length.max(text.len());
                    entries.push(CommandSuggestionEntry {
                        text,
                        tooltip: None,
                    });
                }

                play::clientbound::CommandSuggestions {
                    id: command_suggestion.id,
                    start: (offset + result.start) as i32,
                    length: max_length as i32,
                    entries,
                }.write_packet(&mut self.packet_buffer);
            }
        }

        Ok(())
    }

    fn handle_acknowledge_configuration(&mut self, _: play::serverbound::AcknowledgeConfiguration) -> anyhow::Result<()> {
        bail!("Unsupported packet");
    }

    fn handle_select_trade(&mut self, select_trade: play::serverbound::SelectTrade) -> anyhow::Result<()> {
        let function = self.container_view.do_select_trade(select_trade)?;
        if let Some(function) = function {
            (function)(self);
        }
        Ok(())
    }

    fn handle_client_tick_end(&mut self, _: play::serverbound::ClientTickEnd) -> anyhow::Result<()> {
        const TICK_DURATION_LENIENT: u128 = Duration::from_millis(50).as_nanos() * 999 / 1000;

        if self.player_loaded_time > 0 {
            self.player_loaded_time -= 1;
            return Ok(());
        }

        let now = Instant::now();
        if self.last_client_tick > now {
            self.last_client_tick = now - Duration::from_millis(50);
        }
        let since_last_tick = now - self.last_client_tick;

        let mut pool = self.client_tick_nanos_pool as u128;
        pool += since_last_tick.as_nanos();

        if pool < TICK_DURATION_LENIENT {
            // Client is ticking too fast. Timer cheats?
            // We could probably safely disconnect the client here,
            // but no harm in just silently ignoring the packet
            return Ok(());
        }

        pool -= TICK_DURATION_LENIENT;

        // We allow the client to tick an additional 256 times (~13 seconds)
        // This allows the client to be frozen and then quickly send a bunch of ticks in quick succession
        // 256 ticks was chosen because it's longer than the keepalive duration of 255 ticks
        self.client_tick_nanos_pool = pool.min(TICK_DURATION_LENIENT * 256) as u64;
        self.last_client_tick = now;

        if !self.packets_handled_this_tick.contains(PacketHandledThisTick::Movement) {
            self.update_position_from_client(self.last_client_position)?;
        }
        if self.allow_world_interaction() {
            if let Some(swing_hand) = self.pending_swing_hand.take() {
                P::swing(self, self.last_swing_cause);
                self.last_swing_cause = SwingCause::Miss;
        
                if self.swing_time == 0 {
                    self.swing_time = 3;
                    self.add_viewable_packet(&play::clientbound::AnimateEntity {
                        entity_id: self.entity_id,
                        animation: if swing_hand == InteractionHand::MainHand {
                            EntityAnimation::SwingMainHand
                        } else {
                            EntityAnimation::SwingOffHand
                        },
                    }, PacketViewability::ExcludeSelf);
                }
            }
        } else {
            self.try_abort_using_item();
        }

        if self.camera_entity_id == self.entity_id {
            let forced_pose = calculate_forced_pose(self);
            self.known_client_state.forced_pose = forced_pose;

            if let Some(pose) = calculate_pose(self, forced_pose) {
                self.metadata.set_pose(pose);
                self.known_client_state.pose = pose;
            }
        }

        self.last_swing_cause = SwingCause::Miss;
        self.pending_swing_hand = None;
        self.shift_pressed_last_tick = self.shift_pressed;
        self.sprinting_last_tick = (self.known_client_state.shared_flags & (1 << 3)) != 0;
        self.packets_handled_this_tick.clear();
        self.known_client_state.is_flying_pre_tick = self.known_client_state.is_flying;

        Ok(())
    }

    fn handle_client_information(&mut self, information: play::serverbound::ClientInformation) -> anyhow::Result<()> {
        self.language = Language::try_from(information.language).unwrap_or_default();

        let model_customization = information.model_customization as u8;
        if self.metadata.player_mode_customisation != model_customization {
            self.metadata.set_player_mode_customisation(model_customization);
        }
        let arm_position = match information.arm_position {
            HumanoidArm::Left => 0,
            HumanoidArm::Right => 1,
        };
        if self.metadata.player_main_hand != arm_position {
            self.metadata.set_player_main_hand(arm_position);
        }

        Ok(())
    }

    fn handle_move_player_pos(&mut self, move_player: MovePlayerPos) -> anyhow::Result<()> {
        if self.known_client_state.awaiting_absolute_teleport_count > 0 {
            return Ok(());
        }

        self.last_client_on_ground = move_player.on_ground;
        self.last_client_horizontal_collision = move_player.horizontal_collision;

        if !self.packets_handled_this_tick.contains(PacketHandledThisTick::Movement) {
            let last = self.last_client_position;
            if last.x != move_player.x || last.y != move_player.y || last.z != move_player.z {
                self.update_position_from_client(DVec3::new(move_player.x, move_player.y, move_player.z))?;
            }
        }

        Ok(())
    }

    fn handle_move_player_on_ground(&mut self, move_player: play::serverbound::MovePlayerOnGround) -> anyhow::Result<()> {
        if self.known_client_state.awaiting_absolute_teleport_count > 0 {
            return Ok(());
        }

        self.last_client_on_ground = move_player.on_ground;
        self.last_client_horizontal_collision = move_player.horizontal_collision;
        Ok(())
    }

    fn handle_move_player_pos_rot(&mut self, move_player: MovePlayerPosRot) -> anyhow::Result<()> {
        self.update_rotation_from_client(move_player.yaw, move_player.pitch)?;

        if self.known_client_state.awaiting_absolute_teleport_count > 0 {
            return Ok(());
        }

        let last = self.last_client_position;
        if last.x == move_player.x && last.y == move_player.y && last.z == move_player.z && !move_player.on_ground && !move_player.horizontal_collision {
            // This is probably the dummy PosRot packet the client sends before accepting a teleport, lets ignore it
            return Ok(());
        }

        self.last_client_on_ground = move_player.on_ground;
        self.last_client_horizontal_collision = move_player.horizontal_collision;

        if !self.packets_handled_this_tick.contains(PacketHandledThisTick::Movement) {
            if last.x != move_player.x || last.y != move_player.y || last.z != move_player.z {
                self.update_position_from_client(DVec3::new(move_player.x, move_player.y, move_player.z))?;
            }
        }

        Ok(())
    }

    fn handle_move_player_rot(&mut self, move_player: MovePlayerRot) -> anyhow::Result<()> {
        self.update_rotation_from_client(move_player.yaw, move_player.pitch)?;

        if self.known_client_state.awaiting_absolute_teleport_count > 0 {
            return Ok(());
        }

        self.last_client_on_ground = move_player.on_ground;
        self.last_client_horizontal_collision = move_player.horizontal_collision;

        Ok(())
    }
}

fn is_invalid_movement(x: f64, y: f64, z: f64) -> bool {
    return !x.is_finite() || !y.is_finite() || !z.is_finite();
}

fn is_invalid_rotation(yaw: f32, pitch: f32) -> bool {
    return !yaw.is_finite() || !pitch.is_finite() || pitch < -90.0 || pitch > 90.0;
}

struct KnownBlockGetter<'a, W: WorldExtension> {
    world: &'a World<W>,
    known_changes: &'a FxHashMap<IVec3, u16>,
}

impl <'a, W: WorldExtension> Clone for KnownBlockGetter<'a, W> {
    fn clone(&self) -> Self {
        Self { world: self.world, known_changes: self.known_changes }
    }
}
impl <'a, W: WorldExtension> Copy for KnownBlockGetter<'a, W> {}

impl <'a, W: WorldExtension> KnownBlockGetter<'a, W> {
    pub fn new<P: PlayerExtension<World = W>>(player: &'a Player<P>) -> Self {
        Self {
            world: player.world(),
            known_changes: &player.known_client_state.known_blocks
        }
    }

    pub fn get_block(self, x: i32, y: i32, z: i32) -> Option<u16> {
        if let Some(block) = self.known_changes.get(&IVec3::new(x, y, z)) {
            Some(*block)
        } else {
            self.world.get_block_for_client(x, y, z)
        }
    }
}

fn vanilla_shape_cast<W: WorldExtension>(getter: KnownBlockGetter<W>, mut aabb: AABB, mut delta: DVec3) -> DVec3 {
    if delta.length_squared() == 0.0 {
        return DVec3::ZERO;
    }

    if delta.y != 0.0 {
        delta.y = collide_axis(getter, aabb, 1, delta.y, 1E-7);
        aabb = aabb.translate(DVec3::new(0.0, delta.y, 0.0));
    }

    let prioritize_z = delta.z.abs() > delta.x.abs();
    if prioritize_z && delta.z != 0.0 {
        delta.z = collide_axis(getter, aabb, 2, delta.z, 1E-7);
        aabb = aabb.translate(DVec3::new(0.0, 0.0, delta.z));
    }

    if delta.x != 0.0 {
        delta.x = collide_axis(getter, aabb, 0, delta.x, 1E-7);
        aabb = aabb.translate(DVec3::new(delta.x, 0.0, 0.0));
    }

    if !prioritize_z && delta.z != 0.0 {
        delta.z = collide_axis(getter, aabb, 2, delta.z, 1E-7);
    }

    delta
}

fn collide_axis<W: WorldExtension>(getter: KnownBlockGetter<W>, aabb: AABB, axis: usize, mut length: f64, backwards_threshold: f64) -> f64 {
    if length.abs() < 1.0E-7 {
        return 0.0;
    }
    let backwards_threshold = backwards_threshold.max(0.0);

    // OPTIMIZATION: We offset by -1/+1, can we avoid for every axis other than -Y?

    let cross_axis1 = (axis + 1) % 3;
    let cross_axis2 = (axis + 2) % 3;
    let cross_min1 = aabb.min()[cross_axis1] + 1E-7;
    let cross_max1 = aabb.max()[cross_axis1] - 1E-7;
    let cross_min2 = aabb.min()[cross_axis2] + 1E-7;
    let cross_max2 = aabb.max()[cross_axis2] - 1E-7;
    let cross_range1 = (cross_min1.floor() as i32 - 1) .. (cross_max1.floor() as i32 + 1);
    let cross_range2 = (cross_min2.floor() as i32 - 1) .. (cross_max2.floor() as i32 + 1);
    let (start, mut stop, step, aabb_face) = if length < 0.0 {
        ((aabb.min()[axis] + backwards_threshold).floor() as i32 + 1, (aabb.min()[axis] + length).floor() as i32 - 1, -1, aabb.min()[axis])
    } else {
        ((aabb.max()[axis] - backwards_threshold).floor() as i32 - 1, (aabb.max()[axis] + length).floor() as i32 + 1, 1, aabb.max()[axis])
    };

    // for chunk_x in ((aabb.min().x.floor() as i32 - 8) >> 4) ..= ((aabb.max().x.floor() as i32 + 8) >> 4) {
    //     for chunk_z in ((aabb.min().z.floor() as i32 - 8) >> 4) ..= ((aabb.max().z.floor() as i32 + 8) >> 4) {
    //         let Some(chunk) = world.get_chunk(chunk_x, chunk_z) else {
    //             continue;
    //         };

    //         for solid_aabb in &chunk.solid_entity_aabbs {
    //             if cross_min1 > solid_aabb.max()[cross_axis1] {
    //                 continue;
    //             }
    //             if cross_max1 < solid_aabb.min()[cross_axis1] {
    //                 continue;
    //             }
    //             if cross_min2 > solid_aabb.max()[cross_axis2] {
    //                 continue;
    //             }
    //             if cross_max2 < solid_aabb.min()[cross_axis2] {
    //                 continue;
    //             }

    //             if step < 0 {
    //                 let block_face = solid_aabb.max()[axis];
    //                 let delta = block_face - aabb_face;
    //                 if delta <= backwards_threshold {
    //                     if delta > length {
    //                         length = delta;
    //                         stop = stop.max((aabb.min()[axis] + length).floor() as i32 - 1);
    //                     }
    //                 }
    //             } else {
    //                 let block_face = solid_aabb.min()[axis];
    //                 let delta = block_face - aabb_face;
    //                 if delta >= -backwards_threshold {
    //                     if delta < length {
    //                         length = delta;
    //                         stop = stop.min((aabb.max()[axis] + length).floor() as i32 + 1);
    //                     }
    //                 }
    //             }
    //         }
    //     }   
    // }

    let mut position = start;
    loop {
        for pos1 in cross_range1.clone() {
            for pos2 in cross_range2.clone() {
                let (x, y, z) = match axis {
                    0 => (position, pos1, pos2),
                    1 => (pos2, position, pos1),
                    2 => (pos1, pos2, position),
                    _ => unreachable!()
                };

                let Some(block) = getter.get_block(x, y, z) else {
                    continue;
                };

                if block == 0 {
                    continue;
                }

                let attr: &BlockAttributes = BlockAttributes::from_block_state(block);
                for shape in attr.collision_shape {
                    if cross_min1 > pos1 as f64 + shape[cross_axis1+3] {
                        continue;
                    }
                    if cross_max1 < pos1 as f64 + shape[cross_axis1] {
                        continue;
                    }
                    if cross_min2 > pos2 as f64 + shape[cross_axis2+3] {
                        continue;
                    }
                    if cross_max2 < pos2 as f64 + shape[cross_axis2] {
                        continue;
                    }

                    if step < 0 {
                        let block_face = position as f64 + shape[axis + 3];
                        let delta = block_face - aabb_face;
                        if delta <= backwards_threshold {
                            if delta > length {
                                length = delta;
                                stop = stop.max((aabb.min()[axis] + length).floor() as i32 - 1);
                            }
                        }
                    } else {
                        let block_face = position as f64 + shape[axis];
                        let delta = block_face - aabb_face;
                        if delta >= -backwards_threshold {
                            if delta < length {
                                length = delta;
                                stop = stop.min((aabb.max()[axis] + length).floor() as i32 + 1);
                            }
                        }
                    }
                }                    
            }   
        }

        if step < 0 {
            if position <= stop {
                return length;
            }
        } else {
            if position >= stop {
                return length;
            }
        }
        position += step;
    }
}

struct MovementPrediction {
    movement: DVec3,
    velocity: UncertainVelocity,
    on_ground: bool,
    horizontal_collision: bool,
    main_supporting_block: Option<IVec3>,
    no_jump_delay: u8,
    stuck_multiplier: DVec3,
    is_swimming: bool,
    fluid_heights: FluidHeights,
    reset_fall_distance: bool
}

fn get_fluid<W: WorldExtension>(getter: KnownBlockGetter<W>, x: i32, y: i32, z: i32) -> (u16, Fluid) {
    let Some(block_id) = getter.get_block(x, y, z) else {
        return (0, Fluid::Empty)
    };

    let fluid_state = BlockAttributes::from_block_state(block_id).fluid_state;
    if fluid_state == 0 {
        return (0, Fluid::Empty);
    }

    let Ok(fluid) = graphite_mc_constants::fluid::state_to_fluid(fluid_state) else {
        return (0, Fluid::Empty);
    };
    (fluid_state, fluid)
}

fn is_same_fluid(fluid1: Fluid, fluid2: Fluid) -> bool {
    match fluid1 {
        Fluid::Empty => fluid2 == Fluid::Empty,
        Fluid::FlowingWater | Fluid::Water => fluid2 == Fluid::FlowingWater || fluid2 == Fluid::Water,
        Fluid::FlowingLava | Fluid::Lava => fluid2 == Fluid::FlowingLava || fluid2 == Fluid::Lava,
    }
}

fn get_fluid_height_and_do_fluid_pushing<P: PlayerExtension>(player: &Player<P>, position: DVec3, velocity: &mut UncertainVelocity, water_strength: f64, lava_strength: f64) -> FluidHeights {
    let Some(aabb) = player.create_collision_aabb_at(position).deflate(0.001) else {
        return FluidHeights {
            water: None,
            lava: None,
        }
    };
    let getter = KnownBlockGetter::new(player);

    let min = aabb.min().floor().as_ivec3();
    let max = aabb.max().ceil().as_ivec3();

    let mut water_height: Option<f64> = None;
    let mut lava_height: Option<f64> = None;

    let mut water_flow = DVec3::ZERO;
    let mut water_flow_count = 0;
    let mut lava_flow = DVec3::ZERO;
    let mut lava_flow_count = 0;

    for x in min.x .. max.x {
        for y in min.y .. max.y {
            for z in min.z .. max.z {
                let (fluid_state, fluid) = get_fluid(getter, x, y, z);
                if fluid == Fluid::Empty {
                    continue;
                }

                let (_, above_fluid) = get_fluid(getter, x, y+1, z);

                let fluid_attributes = FluidAttributes::from_fluid_state(fluid_state);
                let fluid_block_height = if is_same_fluid(fluid, above_fluid) {
                    1.0
                } else {
                    fluid_attributes.own_height
                };

                let fluid_y = (y as f32 + fluid_block_height) as f64;
                if fluid_y < aabb.min().y {
                    continue;
                }

                let fluid_height = fluid_y - aabb.min().y;

                let height = match fluid {
                    Fluid::Empty => unreachable!(),
                    Fluid::FlowingWater | Fluid::Water => &mut water_height,
                    Fluid::FlowingLava | Fluid::Lava => &mut lava_height,
                };

                if let Some(height) = height {
                    *height = height.max(fluid_height);
                } else {
                    *height = Some(fluid_height);
                }

                let fluid_falling = match FluidState::try_from(fluid_state).unwrap() {
                    FluidState::Empty {  } => false,
                    FluidState::FlowingWater { falling, level: _ } => falling,
                    FluidState::Water { falling } => falling,
                    FluidState::FlowingLava { falling, level: _ } => falling,
                    FluidState::Lava { falling } => falling,
                };

                let mut flow = get_flow(getter, x, y, z, fluid, fluid_attributes.own_height, fluid_falling);

                if height.unwrap() < 0.4 {
                    flow *= height.unwrap();
                }

                match fluid {
                    Fluid::Empty => unreachable!(),
                    Fluid::FlowingWater | Fluid::Water => {
                        water_flow += flow;
                        water_flow_count += 1;
                    },
                    Fluid::FlowingLava | Fluid::Lava => {
                        lava_flow += flow;
                        lava_flow_count += 1;
                    },
                }
            }
        }
    }

    if water_flow_count > 0 {
        water_flow *= 1.0 / (water_flow_count as f64);
        water_flow *= water_strength;
        apply_flow(velocity, water_flow);
    }

    if lava_flow_count > 0 {
        lava_flow *= 1.0 / (lava_flow_count as f64);
        lava_flow *= lava_strength;
        apply_flow(velocity, lava_flow);
    }

    FluidHeights {
        water: water_height,
        lava: lava_height,
    }
}

fn apply_flow(velocity: &mut UncertainVelocity, flow: DVec3) {
    if flow == DVec3::ZERO {
        return;
    }

    if flow.length() >= 0.0045000000000000005 {
        *velocity += flow;
    } else {
        let mut min = velocity.get_min();
        let mut max = velocity.get_max();
        if min.x >= 0.003 || max.x <= -0.003 || min.z >= 0.003 || max.z <= -0.003 {
            // Only possibility is normal flow
            *velocity += flow;
        } else if min.x > -0.003 && max.x < 0.003 && min.z > -0.003 && max.z < 0.003 {
            // Only possiblility is adjusted flow
            *velocity += mojang_math::normalize(flow) * 0.0045000000000000005;
        } else {
            // Need to account for either flow type
            let adjusted_flow = mojang_math::normalize(flow) * 0.0045000000000000005;

            let mut known = velocity.get_expected(); 
            if known.x.abs() < 0.003 && known.z.abs() < 0.003 {
                known += adjusted_flow;
            } else {
                known += flow;
            }

            for axis in 0..3 {
                if flow[axis] < 0.0 {
                    min[axis] += adjusted_flow[axis];
                    max[axis] += flow[axis];
                } else {
                    min[axis] += flow[axis];
                    max[axis] += adjusted_flow[axis];
                }
            }

            velocity.set_with_min_max(known, min, max);
        }
    }
}

fn get_flow<W: WorldExtension>(getter: KnownBlockGetter<W>, x: i32, y: i32, z: i32, fluid: Fluid, fluid_own_height: f32, mut fluid_falling: bool) -> DVec3 {
    let mut flow_x = 0.0;
    let mut flow_z = 0.0;

    for direction_index in 0..4 {
        let (offset_x, offset_z) = match direction_index {
            0 => (0, -1), // north
            1 => (1, 0), // east
            2 => (0, 1), // south
            3 => (-1, 0), // west
            _ => unreachable!()
        };

        let Some(neighbor_block_id) = getter.get_block(x+offset_x, y, z+offset_z) else {
            fluid_falling = false;
            continue;
        };
    
        let neighbor_fluid_state = BlockAttributes::from_block_state(neighbor_block_id).fluid_state;
        let neighbor_fluid = if neighbor_fluid_state == 0 {
            Fluid::Empty
        } else {
            graphite_mc_constants::fluid::state_to_fluid(neighbor_fluid_state).unwrap_or(Fluid::Empty)
        };

        if fluid_falling {
            if is_solid_face(fluid, neighbor_block_id, neighbor_fluid, direction_index) {
                fluid_falling = false;
            } else if let Some(above_neighbor_block_id) = getter.get_block(x+offset_x, y+1, z+offset_z) {
                if above_neighbor_block_id != neighbor_block_id {
                    let above_neighbor_fluid_state = BlockAttributes::from_block_state(above_neighbor_block_id).fluid_state;
                    let above_neighbor_fluid = if above_neighbor_fluid_state == 0 {
                        Fluid::Empty
                    } else {
                        graphite_mc_constants::fluid::state_to_fluid(above_neighbor_fluid_state).unwrap_or(Fluid::Empty)
                    };

                    if is_solid_face(fluid, above_neighbor_block_id, above_neighbor_fluid, direction_index) {
                        fluid_falling = false;
                    }
                }
            }
        }

        if neighbor_fluid != Fluid::Empty && !is_same_fluid(fluid, neighbor_fluid) {
            continue;
        }

        let mut strength = 0.0;

        let neighbor_attrs = graphite_mc_constants::fluid::FluidAttributes::from_fluid_state(neighbor_fluid_state);
        if neighbor_attrs.own_height == 0.0 {
            if !graphite_mc_constants::block::BlockAttributes::from_block_state(neighbor_block_id).has_flag(BlockFlag::BlocksMotion) {
                let (below_fluid_state, below_fluid) = get_fluid(getter, x+offset_x, y-1, z+offset_z);

                if neighbor_fluid == Fluid::Empty || is_same_fluid(fluid, below_fluid) {
                    let below_attrs = graphite_mc_constants::fluid::FluidAttributes::from_fluid_state(below_fluid_state);
                    if below_attrs.own_height > 0.0 {
                        strength = fluid_own_height - (below_attrs.own_height - 0.8888889_f32);
                    }
                }
            }
        } else if neighbor_attrs.own_height > 0.0 {
            strength = fluid_own_height - neighbor_attrs.own_height;
        }

        flow_x += (offset_x as f32 * strength) as f64;
        flow_z += (offset_z as f32 * strength) as f64;
    }

    let mut flow = DVec3::new(flow_x, 0.0, flow_z);

    if fluid_falling {
        flow = mojang_math::normalize(flow);
        flow.y -= 6.0;
    }

    mojang_math::normalize(flow)
}

fn is_solid_face(fluid: Fluid, neighbor_block_id: u16, neighbor_fluid: Fluid, direction_index: usize) -> bool {
    const ICE: u16 = graphite_mc_constants::block::BlockState::Ice {  }.to_id();
    const FROSTED_ICE0: u16 = graphite_mc_constants::block::BlockState::FrostedIce { age: 0 }.to_id();
    const FROSTED_ICE1: u16 = graphite_mc_constants::block::BlockState::FrostedIce { age: 1 }.to_id();
    const FROSTED_ICE2: u16 = graphite_mc_constants::block::BlockState::FrostedIce { age: 2 }.to_id();
    const FROSTED_ICE3: u16 = graphite_mc_constants::block::BlockState::FrostedIce { age: 3 }.to_id();

    if is_same_fluid(fluid, neighbor_fluid) {
        return false;
    } else if neighbor_block_id == 0 || neighbor_block_id == ICE || neighbor_block_id == FROSTED_ICE0 ||
            neighbor_block_id == FROSTED_ICE1 || neighbor_block_id == FROSTED_ICE2 || neighbor_block_id == FROSTED_ICE3 {
        return false;
    } else {
        let neighbor_block_attrs = graphite_mc_constants::block::BlockAttributes::from_block_state(neighbor_block_id);

        // Logically speaking, the direction should be inverted, but that's not
        // what Notch did 10 years ago so I guess this is how it is
        return match direction_index {
            0 => neighbor_block_attrs.has_flag(BlockFlag::IsSturdyNorth),
            1 => neighbor_block_attrs.has_flag(BlockFlag::IsSturdyEast),
            2 => neighbor_block_attrs.has_flag(BlockFlag::IsSturdySouth),
            3 => neighbor_block_attrs.has_flag(BlockFlag::IsSturdyWest),
            _ => unreachable!()
        };
    }
            
}

fn check_velocity(client: DVec3, server: &UncertainVelocity, title: &'static str) {
    if !crate::debug::DEBUG_MOVEMENT {
        return;
    }

    let min = server.get_min();
    let max = server.get_max();
    if client.x < min.x || client.y < min.y || client.z < min.z || client.x > max.x || client.y > max.y || client.z > max.z  {
        println!("Velocity mismatch ({})", title);
        println!("Client: {:?}", client);
        if min == max {
            println!("Server: {:?}", min);
        } else {
            println!("Server: {:?} to {:?}", min, max);
        }
    }
}

#[must_use]
fn predict_movement<P: PlayerExtension>(player: &Player<P>, client_position: DVec3, trust_client: bool) -> MovementPrediction {
    // Inputs
    let is_using_item = (player.known_client_state.living_flags & 1) != 0;
    let is_sprinting = (player.known_client_state.shared_flags & (1 << 3)) != 0;
    let mut is_swimming = (player.known_client_state.shared_flags & (1 << 4)) != 0;
    let mut is_flying = player.known_client_state.is_flying_pre_tick;
    let is_fall_flying = (player.known_client_state.shared_flags & (1 << 7)) != 0;
    let flying_speed = player.known_client_state.flying_speed as f32;
    let sneaking_speed = player.known_client_state.sneaking_speed;
    let movement_speed = player.known_client_state.movement_speed as f32;
    let gravity = player.known_client_state.gravity;
    let pose = player.known_client_state.pose;

    let effects: EnumMap<MobEffect, Option<i32>> = player.known_client_state.effects;

    let mut velocity = player.known_client_state.velocity.clone();
    let forwards = player.forwards_input as f32;
    let left = -player.strafe_input as f32;
    let mut no_jump_delay = player.no_jump_delay;
    let mut reset_fall_distance = false;
    let mut stuck_multiplier = player.stuck_multiplier;

    let getter = KnownBlockGetter::new(player);

    check_velocity(player.known_client_state.debug_state.base_tick_velocity, &velocity, "base tick");

    // updateIsUnderwater
    let is_under_water = is_same_fluid(Fluid::Water, player.fluid_at_eyes); // Note: Player #isUnderWater removes is_in_water condition

    // updateInWaterStateAndDoFluidPushing
    let fluid_heights = if is_flying {
        get_fluid_height_and_do_fluid_pushing(player, player.last_client_position, &mut velocity, 0.0, 0.0)
    } else {
        get_fluid_height_and_do_fluid_pushing(player, player.last_client_position, &mut velocity, 0.014, 0.0023333333333333335)
    };
    let is_in_water = fluid_heights.water.is_some();
    let is_in_lava = fluid_heights.lava.is_some() && fluid_heights.lava.unwrap() > 0.0;

    reset_fall_distance |= is_in_water;

    if crate::debug::DEBUG_MOVEMENT && is_in_water != player.known_client_state.debug_state.is_in_water {
        println!("is_in_water is wrong. Server: {:?}. Client: {:?}. Position: {:?}", is_in_water, player.known_client_state.debug_state.is_in_water, player.last_client_position);
    }
    if crate::debug::DEBUG_MOVEMENT && is_in_lava != player.known_client_state.debug_state.is_in_lava {
        println!("is_in_lava is wrong. Server: {:?}. Client: {:?}. Position: {:?}", is_in_lava, player.known_client_state.debug_state.is_in_lava, player.last_client_position);
    }

    // updateSwimming
    // Note: we need to use sprinting_last_tick here because the sprint state is updated later in LocalPlayer#aiStep
    if is_flying {
        is_swimming = false;
    } else if is_swimming {
        is_swimming = player.sprinting_last_tick && is_in_water;
    } else {
        let block_position = player.last_client_position.floor().as_ivec3();
        is_swimming = player.sprinting_last_tick && is_under_water && is_same_fluid(Fluid::Water, get_fluid(getter, block_position.x, block_position.y, block_position.z).1);
    }
    if crate::debug::DEBUG_MOVEMENT && is_swimming != player.known_client_state.debug_state.is_swimming {
        println!("is_swimming is wrong. Server: {:?}. Client: {:?}", is_swimming, player.known_client_state.debug_state.is_swimming);
    }

    // LocalPlayer aiStep
    check_velocity(player.known_client_state.debug_state.local_player_ai_step_velocity, &velocity, "local player ai step");
    
    let forced_pose = if player.camera_entity_id == player.entity_id {
        calculate_forced_pose(player)
    } else {
        ForcedPose::None
    };

    // Note: We need to use shift_pressed_last_tick because the key presses are updated after calculating is_crouching
    let is_crouching = !is_flying && !is_swimming && forced_pose.can_fit_when(Pose::Crouching) &&
        (player.shift_pressed_last_tick || !forced_pose.can_fit_when(Pose::Standing));
    let is_visually_swimming = (pose == Pose::Swimming && !is_in_water) || (!is_fall_flying && pose == Pose::FallFlying);
    let is_visually_crawling = is_visually_swimming && !is_in_water;
    let is_moving_slowly = is_crouching || is_visually_crawling;
    let moving_slowly_speed = if is_moving_slowly {
        Some(player.known_client_state.sneaking_speed as f32)
    } else {
        None
    };

    let bb_width = match pose {
        Pose::Sleeping | Pose::Dying => 0.2_f32,
        _ => 0.6_f32,
    } as f64;

    let mut suffocation_cache = HashMap::new();
    move_towards_closest_space(player, &mut velocity, &mut suffocation_cache, -bb_width * 0.35, bb_width * 0.35);
    move_towards_closest_space(player, &mut velocity, &mut suffocation_cache, -bb_width * 0.35, -bb_width * 0.35);
    move_towards_closest_space(player, &mut velocity, &mut suffocation_cache, bb_width * 0.35, -bb_width * 0.35);
    move_towards_closest_space(player, &mut velocity, &mut suffocation_cache, bb_width * 0.35, bb_width * 0.35);

    if is_flying != player.known_client_state.is_flying {
        // Potentially need to toggle flying here, could be due to double-jump (here) or landing on ground (later)
        let disabled_because_on_ground = player.last_client_on_ground && !player.known_client_state.is_flying;
        if !is_swimming && player.jump_pressed && !disabled_because_on_ground {
            is_flying = player.known_client_state.is_flying;

            if is_flying && player.on_ground {
                jump_from_ground(player, is_sprinting, effects, &mut velocity);
            }
        }
    }

    let is_affected_by_fluids = !is_flying;
    if is_in_water && player.shift_pressed && is_affected_by_fluids {
        velocity.add_y(-0.03999999910593033);
    }
    if is_flying {
        let mut up = 0.0;
        if player.shift_pressed {
            up -= 1.0;
        }
        if player.jump_pressed {
            up += 1.0;
        }
        velocity.add_y((up * flying_speed * 3.0) as f64);
    }

    // Player aiStep
    if is_flying {
        reset_fall_distance = true;
    }

    // LivingEntity aiStep
    check_velocity(player.known_client_state.debug_state.living_ai_step_velocity, &velocity, "living ai step");

    if no_jump_delay > 0 && no_jump_delay != u8::MAX {
        no_jump_delay -= 1;
    }

    // applyInput moved to move_relative

    { // 0.003 threshold
        let min = velocity.get_min();
        let max = velocity.get_max();
        let least_x = 0.0_f64.clamp(min.x, max.x);
        let least_z = 0.0_f64.clamp(min.z, max.z);
        if least_x*least_x + least_z*least_z < 9.0e-6 {
            if velocity.has_uncertainty() {
                let expected = velocity.get_expected();
                if expected.x*expected.x + expected.z*expected.z < 9.0e-6 {
                    velocity.set_with_min_max(DVec3::new(0.0, expected.y, 0.0), min, max);
                } else {
                    velocity.expand_xz_uncertainty_to_zero();
                }
            } else {
                velocity.set_x(0.0);
                velocity.set_z(0.0);
            }
        }
        if velocity.get_y().abs() < 0.003 {
            velocity.set_y(0.0);
        }
    }

    let fluid_jump_threshold = 0.4;

    if player.jump_pressed && is_affected_by_fluids {
        let fluid_height;
        if is_in_lava {
            fluid_height = fluid_heights.lava.unwrap_or(0.0);
        } else {
            fluid_height = fluid_heights.water.unwrap_or(0.0);
        }

        let in_water = is_in_water && fluid_height > 0.0;

        // Only allow jump if no_jump_delay is zero
        // However, sometimes the jump delay is unknown (eg. because of a pending teleport)
        // In which case we fallback to jumping if the client says it has left the ground (which works in most but not all cases)
        let can_jump_on_ground = no_jump_delay == 0 || (no_jump_delay == u8::MAX && !player.last_client_on_ground);

        if in_water && !(player.on_ground && fluid_height <= fluid_jump_threshold) {
            // Jump in water
            velocity.add_y(0.03999999910593033);
        } else if is_in_lava && !(player.on_ground && fluid_height <= fluid_jump_threshold) {
            // Jump in lava
            velocity.add_y(0.03999999910593033);
        } else if (player.on_ground || (in_water && fluid_height <= fluid_jump_threshold)) && can_jump_on_ground {
            jump_from_ground(player, is_sprinting, effects, &mut velocity);
            no_jump_delay = 10;
        }
    } else {
        no_jump_delay = 0;
    }

    if effects[MobEffect::SlowFalling].is_some() || effects[MobEffect::Levitation].is_some() {
         reset_fall_distance = true;
    }

    // Player#travel
    if is_swimming {
        let look = -mojang_math::sin(player.pitch * 0.017453292_f32) as f64;
        let strength = if look < -0.2 {
            0.085
        } else {
            0.06
        };
        let above = DVec3::new(player.last_client_position.x, player.last_client_position.y + 1.0 - 0.1, player.last_client_position.z);
        let above_block = above.floor().as_ivec3();
        if look <= 0.0 || player.jump_pressed || !is_same_fluid(Fluid::Empty, get_fluid(getter, above_block.x, above_block.y, above_block.z).1) {
            velocity.add_y((look - velocity.get_y()) * strength);
        }
    }

    let before_travel_velocity_y = velocity.get_y();

    check_velocity(player.known_client_state.debug_state.travel_velocity, &velocity, "travel");

    let prediction;

    if (is_in_water || is_in_lava) && is_affected_by_fluids {
        let was_descending = velocity.get_y() < 0.0;

        if is_in_water {
            let mut slowdown = if is_sprinting {
                0.9
            } else {
                0.8
            };
            let mut speed = 0.02_f32;

            let mut efficiency = player.known_client_state.water_movement_efficiency as f32;
            if !player.on_ground {
                efficiency *= 0.5;
            }
            if efficiency > 0.0 {
                slowdown += (0.54600006_f32 - slowdown) * efficiency;
                speed += (movement_speed - speed) * efficiency;
            }

            if effects[MobEffect::DolphinsGrace].is_some() {
                slowdown = 0.96_f32;
            }

            if crate::debug::DEBUG_MOVEMENT && speed != player.known_client_state.debug_state.move_relative_speed {
                println!("Move relative speed is wrong. Server: {:?}. Client: {:?}", speed, player.known_client_state.debug_state.move_relative_speed);
            }
            check_velocity(player.known_client_state.debug_state.move_relative_velocity, &velocity, "move relative");
            move_relative(player.yaw, left, forwards, speed, &mut velocity, client_position - player.last_client_position,
                is_using_item, moving_slowly_speed, player.known_client_state.debug_state.move_inputs);

            // Move
            check_velocity(player.known_client_state.debug_state.move_velocity, &velocity, "move");
            prediction = finalize_movement(player, trust_client, velocity, stuck_multiplier, is_flying, client_position, is_in_water);
            velocity = prediction.velocity;
            stuck_multiplier = prediction.stuck_multiplier;

            let new_client_position = move_towards(player.last_client_position + prediction.movement, client_position, 0.25);
            if prediction.horizontal_collision && !is_flying && is_on_climbable(getter, new_client_position) {
                velocity.set_y(0.2);
            }

            velocity *= DVec3::new(slowdown as f64, 0.800000011920929, slowdown as f64);

            // getFluidFallingAdjustedMovement
            if gravity != 0.0 && !is_sprinting {
                if was_descending && (velocity.get_y() - 0.005).abs() >= 0.003 && (velocity.get_y() - gravity / 16.0).abs() < 0.003 {
                    velocity.set_y(-0.003);
                } else {
                    velocity.add_y(-gravity / 16.0);
                }
            }
        } else {
            if crate::debug::DEBUG_MOVEMENT && 0.02_f32 != player.known_client_state.debug_state.move_relative_speed {
                println!("Move relative speed is wrong. Server: {:?}. Client: {:?}", 0.02_f32, player.known_client_state.debug_state.move_relative_speed);
            }
            check_velocity(player.known_client_state.debug_state.move_relative_velocity, &velocity, "move relative");
            move_relative(player.yaw, left, forwards, 0.02_f32, &mut velocity, client_position - player.last_client_position,
                is_using_item, moving_slowly_speed, player.known_client_state.debug_state.move_inputs);

            // Move
            check_velocity(player.known_client_state.debug_state.move_velocity, &velocity, "move");
            prediction = finalize_movement(player, trust_client, velocity, stuck_multiplier, is_flying, client_position, is_in_water);
            velocity = prediction.velocity;
            stuck_multiplier = prediction.stuck_multiplier;

            if fluid_heights.lava.unwrap_or(0.0) <= fluid_jump_threshold {
                velocity *= DVec3::new(0.5, 0.8_f32 as f64, 0.5);

                // getFluidFallingAdjustedMovement
                if gravity != 0.0 && !is_sprinting {
                    if was_descending && (velocity.get_y() - 0.005).abs() >= 0.003 && (velocity.get_y() - gravity / 16.0).abs() < 0.003 {
                        velocity.set_y(-0.003);
                    } else {
                        velocity.add_y(-gravity / 16.0);
                    }
                }
            } else {
                velocity *= 0.5;
            }

            velocity.add_y(-gravity / 4.0);
        }

        if prediction.horizontal_collision {
            let new_client_position = move_towards(player.last_client_position + prediction.movement, client_position, 0.25);
            let mut aabb = player.create_collision_aabb_at(new_client_position);

            let expected = velocity.get_expected();
            aabb = aabb.translate(DVec3::new(
                expected.x,
                expected.y + 0.6_f32 as f64 - new_client_position.y + player.last_client_position.y,
                expected.z
            ));
            if is_free(getter, aabb, true, false) {
                velocity.set_y(0.3_f32 as f64);
            }
        }
    } else if is_fall_flying {
        // Travel fall flying
        // todo: cancel if on climbable!
        let pitch_rads = player.pitch * 0.017453292_f32;
		let yaw_rads = -player.yaw * 0.017453292_f32;
		let cos_yaw = mojang_math::cos(yaw_rads);
		let sin_yaw = mojang_math::sin(yaw_rads);
		let cos_pitch = mojang_math::cos(pitch_rads);
		let sin_pitch = mojang_math::sin(pitch_rads);
		let look =  DVec3::new((sin_yaw * cos_pitch) as f64, -sin_pitch as f64, (cos_yaw * cos_pitch) as f64);
        let horz_look = (look.x * look.x + look.z * look.z).sqrt();

        // note: cos() is platform-dependent. This causes slight inaccuracies between the client and server. Too bad
        let cos_pitch = (pitch_rads as f64).cos();
        let cos_pitch_sq = cos_pitch * cos_pitch;

        velocity += DVec3::new(0.0, gravity * (-1.0 + cos_pitch_sq * 0.75), 0.0);

        velocity.modify(|vec| {
            let horz_velocity = (vec.x * vec.x + vec.z * vec.z).sqrt();

            if vec.y < 0.0 && horz_look > 0.0 {
                let strength = vec.y * -0.1 * cos_pitch_sq;
                *vec += DVec3::new(look.x * strength / horz_look, strength, look.z * strength / horz_look);
            }

            if pitch_rads < 0.0 && horz_look > 0.0 {
                let strength = horz_velocity * (-sin_pitch as f64) * 0.04;
                *vec += DVec3::new(-look.x * strength / horz_look, strength * 3.2, -look.z * strength / horz_look);
            }
    
            if horz_look > 0.0 {
                *vec += DVec3::new((look.x / horz_look * horz_velocity - vec.x) * 0.1, 0.0, (look.z / horz_look * horz_velocity - vec.z) * 0.1);
            }

            *vec *= DVec3::new(0.99_f32 as f64, 0.98_f32 as f64, 0.99_f32 as f64);
        });
		
        // Move
        check_velocity(player.known_client_state.debug_state.move_velocity, &velocity, "move");
        prediction = finalize_movement(player, trust_client, velocity, stuck_multiplier, is_flying, client_position, is_in_water);
        velocity = prediction.velocity;
        stuck_multiplier = prediction.stuck_multiplier;
    } else {
        // Travel in air
        let friction = if player.on_ground {
            let below_pos = get_block_pos_below_that_affects_my_movement(player.main_supporting_block, player.last_client_position);
            if let Some(block) = getter.get_block(below_pos.x, below_pos.y, below_pos.z) {
                BlockAttributes::from_block_state(block).friction
            } else {
                0.6_f32
            }
        } else {
            1.0_f32
        };
        let friction_influenced_speed = if player.on_ground {
            movement_speed * (0.21600002_f32 / (friction * friction * friction))
        } else if is_flying {
            if is_sprinting {
                flying_speed * 2.0_f32
            } else {
                flying_speed
            }
        } else if is_sprinting {
            0.025999999_f32
        } else {
            0.02_f32
        };

        // Move relative
        if crate::debug::DEBUG_MOVEMENT && friction_influenced_speed != player.known_client_state.debug_state.move_relative_speed {
            println!("Move relative speed is wrong. Server: {:?}. Client: {:?}", friction_influenced_speed, player.known_client_state.debug_state.move_relative_speed);
        }
        if crate::debug::DEBUG_MOVEMENT && is_sprinting != player.known_client_state.debug_state.is_sprinting {
            println!("Sprint state is wrong. Server: {:?}. Client: {:?}", is_sprinting, player.known_client_state.debug_state.is_sprinting);
        }
        if crate::debug::DEBUG_MOVEMENT && player.known_client_state.applied_sprint_modifier_to_movement_speed != player.known_client_state.debug_state.has_sprint_modifier_applied {
            println!("Sprint modifier state is wrong. Sprinting: {:?}. Server: {:?}. Client: {:?}", is_sprinting, player.known_client_state.applied_sprint_modifier_to_movement_speed, player.known_client_state.debug_state.has_sprint_modifier_applied);
        }
        check_velocity(player.known_client_state.debug_state.move_relative_velocity, &velocity, "move relative");
        move_relative(player.yaw, left, forwards, friction_influenced_speed, &mut velocity, client_position - player.last_client_position,
            is_using_item, moving_slowly_speed, player.known_client_state.debug_state.move_inputs);

        if !is_flying && is_on_climbable(getter, player.last_client_position) {
            reset_fall_distance = true;

            velocity.modify(|velocity| {
                velocity.x = velocity.x.clamp(-0.15000000596046448, 0.15000000596046448);
                velocity.z = velocity.z.clamp(-0.15000000596046448, 0.15000000596046448);
            });

            if !is_flying && player.shift_pressed {
                velocity.max_y(0.0);
            } else {
                velocity.max_y(-0.15000000596046448);
            }
        }

        // Move
        check_velocity(player.known_client_state.debug_state.move_velocity, &velocity, "move");
        prediction = finalize_movement(player, trust_client, velocity, stuck_multiplier, is_flying, client_position, is_in_water);
        velocity = prediction.velocity;
        stuck_multiplier = prediction.stuck_multiplier;

        let new_client_position = move_towards(player.last_client_position + prediction.movement, client_position, 0.25);
        if (prediction.horizontal_collision || player.jump_pressed) && !is_flying && is_on_climbable(getter, new_client_position) {
            velocity.set_y(0.2);
        }

        if let Some(levitation_amplifier) = effects[MobEffect::Levitation] {
            velocity.add_y((0.05 * (levitation_amplifier + 1) as f64 - velocity.get_y()) * 0.2);
        } else {
            velocity.add_y(-gravity);
        }

        let resistance = friction * 0.91;
        velocity *= DVec3::new(resistance as f64, 0.98_f32 as f64, resistance as f64);
    }

    check_velocity(player.known_client_state.debug_state.after_travel_velocity, &velocity, "after travel");

    if is_flying {
        velocity.set_y(before_travel_velocity_y * 0.6);
    }

    let new_client_position = move_towards(player.last_client_position + prediction.movement, client_position, 0.25);

    // applyEffectsFromBlocks

    // stepOn
    if prediction.on_ground {
        // There are a few stepOn effects, but the only one that affects movement is the slime block.
        // The !shift_pressed and yVel.abs() < 0.1 checks are preconditions for the slime block slowdown, we do it first to avoid the block lookup
        if !player.shift_pressed && velocity.get_y().abs() < 0.1 {
            let on_pos_legacy = get_on_pos_legacy(prediction.main_supporting_block, new_client_position);
            let on_block_state = getter.get_block(on_pos_legacy.x, on_pos_legacy.y, on_pos_legacy.z).unwrap_or(0);
    
            const SLIME_BLOCK: u16 = BlockState::SlimeBlock {  }.to_id();
            if on_block_state == SLIME_BLOCK {
                let factor = 0.4 + velocity.get_y().abs() * 0.2;
                velocity *= DVec3::new(factor, 1.0, factor);
            }
        }
    }
    // checkInsideBlocks
    
    // todo: this changed in 1.21.5, causing issues with blocks like bubble columns
    // switch to vanilla's new code for checking inside blocks

    if player.last_client_position == new_client_position {
        let mut visited = HashSet::new();
        check_inside_blocks(player, player.last_client_position, new_client_position, &mut visited, &mut velocity, &mut reset_fall_distance, &mut stuck_multiplier, prediction.on_ground, is_flying);
    } else {
        let mut from_position = player.last_client_position;
        let mut to_position = player.last_client_position;
        let delta = new_client_position - player.last_client_position;

        let mut visited = HashSet::new();

        if delta.y.abs() != 0.0 {
            to_position.y = new_client_position.y;
            check_inside_blocks(player, from_position, to_position, &mut visited, &mut velocity, &mut reset_fall_distance, &mut stuck_multiplier, prediction.on_ground, is_flying);
            from_position.y = new_client_position.y;
        }

        let prioritize_z = delta.z.abs() > delta.x.abs();
        if prioritize_z && delta.z != 0.0 {
            to_position.z = new_client_position.z;
            check_inside_blocks(player, from_position, to_position, &mut visited, &mut velocity, &mut reset_fall_distance, &mut stuck_multiplier, prediction.on_ground, is_flying);
            from_position.z = new_client_position.z;
        }

        if delta.x != 0.0 {
            to_position.x = new_client_position.x;
            check_inside_blocks(player, from_position, to_position, &mut visited, &mut velocity, &mut reset_fall_distance, &mut stuck_multiplier, prediction.on_ground, is_flying);
            from_position.x = new_client_position.x;
        }

        if !prioritize_z {
            to_position.z = new_client_position.z;
            check_inside_blocks(player, from_position, to_position, &mut visited, &mut velocity, &mut reset_fall_distance, &mut stuck_multiplier, prediction.on_ground, is_flying);
            from_position.z = new_client_position.z;
        }
    }

    MovementPrediction {
        velocity,
        no_jump_delay,
        is_swimming,
        reset_fall_distance,
        stuck_multiplier,
        fluid_heights,
        ..prediction
    }
}

fn jump_from_ground<P: PlayerExtension>(player: &Player<P>, is_sprinting: bool, effects: EnumMap<MobEffect, Option<i32>>, velocity: &mut UncertainVelocity) {
    let block_jump_factor = get_block_jump_factor(player);
    let jump_boost_power = if let Some(jump_boost_level) = effects[MobEffect::JumpBoost] {
        0.1_f32 * (jump_boost_level as f32 + 1.0)
    } else {
        0.0
    };
    let jump_power = player.known_client_state.jump_strength as f32 * block_jump_factor + jump_boost_power;
    let jump_power = jump_power as f64;

    velocity.max_y(jump_power);

    if is_sprinting {
        let sin = mojang_math::sin(player.yaw * 0.017453292_f32);
        let cos = mojang_math::cos(player.yaw * 0.017453292_f32);
        *velocity += DVec3::new(-sin as f64 * 0.2, 0.0, cos as f64 * 0.2);
    }
}

fn get_block_jump_factor<P: PlayerExtension>(player: &Player<P>) -> f32 {
    let getter = KnownBlockGetter::new(player);
    let block_position = player.last_client_position.floor().as_ivec3();
    let current_block = getter.get_block(block_position.x, block_position.y, block_position.z).unwrap_or(0);
    if current_block != 0 {
        let attr = BlockAttributes::from_block_state(current_block);
        if attr.jump_factor != 1.0 {
            return attr.jump_factor;
        }
    }

    let below = get_block_pos_below_that_affects_my_movement(player.main_supporting_block, player.last_client_position);
    if below == block_position {
        return 1.0;
    }
    let below_block = getter.get_block(below.x, below.y, below.z).unwrap_or(0);
    if below_block != 0 && below_block != current_block {
        let attr = BlockAttributes::from_block_state(below_block);
        if attr.jump_factor != 1.0 {
            return attr.jump_factor;
        }
    }

    return 1.0;
}

fn check_inside_blocks<P: PlayerExtension>(player: &Player<P>, from: DVec3, to: DVec3, visited: &mut HashSet<(i32, i32, i32)>, velocity: &mut UncertainVelocity, reset_fall_distance: &mut bool, stuck_multiplier: &mut DVec3, on_ground: bool, flying: bool) {
    let aabb = player.create_collision_aabb_at(to).deflate(1.0E-5_f32 as f64).unwrap();
    let delta = to - from;
    if delta.length_squared() < (0.99999_f32 * 0.99999_f32) as f64 {
        let min = aabb.min().floor().as_ivec3();
        let max = aabb.max().floor().as_ivec3();
        for z in min.z .. max.z+1 {
            for y in min.y .. max.y+1 {
                for x in min.x .. max.x+1 {
                    if visited.insert((x, y, z)) {
                        check_inside_block(player, IVec3::new(x, y, z), velocity, reset_fall_distance, stuck_multiplier, to, on_ground, flying);
                    }
                }
            }
        }
    } else {
        let to_min = aabb.min();
        let from_min = to_min - delta;
        let delta = to_min - from_min;

        let mut map = from_min.floor().as_ivec3();

        let step_x = delta.x.signum() as i32;
        let step_y = delta.y.signum() as i32;
        let step_z = delta.z.signum() as i32;

        let delta_dist_x = if step_x == 0 {
            f64::MAX
        } else {
            step_x as f64 / delta.x
        };
        let delta_dist_y = if step_y == 0 {
            f64::MAX
        } else {
            step_y as f64 / delta.y
        };
        let delta_dist_z = if step_z == 0 {
            f64::MAX
        } else {
            step_z as f64 / delta.z
        };

        let mut side_dist_x = delta_dist_x * if step_x > 0 {
            1.0 - (from_min.x - map.x as f64)
        } else {
            from_min.x - map.x as f64
        };
        let mut side_dist_y = delta_dist_y * if step_y > 0 {
            1.0 - (from_min.y - map.y as f64)
        } else {
            from_min.y - map.y as f64
        };
        let mut side_dist_z = delta_dist_z * if step_z > 0 {
            1.0 - (from_min.z - map.z as f64)
        } else {
            from_min.z - map.z as f64
        };

        let mut steps = 0;

        while side_dist_x <= 1.0 || side_dist_y <= 1.0 || side_dist_z <= 1.0 {
            if side_dist_x < side_dist_y {
                if side_dist_x < side_dist_z {
                    map.x += step_x;
                    side_dist_x += delta_dist_x;
                } else {
                    map.z += step_z;
                    side_dist_z += delta_dist_z;
                }
            } else if delta_dist_y < delta_dist_z {
                map.y += step_y;
                side_dist_y += delta_dist_y;
            } else {
                map.z += step_z;
                side_dist_z += delta_dist_z;
            }

            if steps > 16 {
                break;
            }
            steps += 1;

            if let Some(hit) = aabb_clip(map.as_dvec3(), (map + IVec3::ONE).as_dvec3(), from_min, to_min) {
                let clamp_min = map.as_dvec3() + DVec3::splat(1.0E-5_f32 as f64);
                let clamp_max = map.as_dvec3() + DVec3::splat(1.0) - DVec3::splat(1.0E-5_f32 as f64);
                let inside = hit.clamp(clamp_min, clamp_max);

                let map_to = (inside + (aabb.max() - aabb.min())).floor().as_ivec3();

                for x in map.x .. map_to.x+1 {
                    for y in map.y .. map_to.y+1 {
                        for z in map.z .. map_to.z+1 {
                            if visited.insert((x, y, z)) {
                                check_inside_block(player, IVec3::new(x, y, z), velocity, reset_fall_distance, stuck_multiplier, to, on_ground, flying);
                            }
                        }
                    }
                }
            }
        }

        // Also need to check all blocks inside aabb, as long as they haven't been visited yet
        let min = aabb.min().floor().as_ivec3();
        let max = aabb.max().floor().as_ivec3();
        for z in min.z .. max.z+1 {
            for y in min.y .. max.y+1 {
                for x in min.x .. max.x+1 {
                    if visited.contains(&(x, y, z)) {
                        continue;
                    }
                    check_inside_block(player, IVec3::new(x, y, z), velocity, reset_fall_distance, stuck_multiplier, to, on_ground, flying);
                }
            }
        }
    }
}

fn aabb_clip(min: DVec3, max: DVec3, from: DVec3, to: DVec3) -> Option<DVec3> {
    let delta = to - from;

    let mut delta_scale = 1.0;

    if delta.x > 1.0E-7 {
        clip_point(&mut delta_scale, delta.x, delta.y, delta.z, min.x, min.y, max.y, min.z, max.z, from.x, from.y, from.z);
    } else if delta.x < -1.0E-7 {
        clip_point(&mut delta_scale, delta.x, delta.y, delta.z, max.x, min.y, max.y, min.z, max.z, from.x, from.y, from.z);
    }

    if delta.y > 1.0E-7 {
        clip_point(&mut delta_scale, delta.y, delta.z, delta.x, min.y, min.z, max.z, min.x, max.x, from.y, from.z, from.x);
    } else if delta.y < -1.0E-7 {
        clip_point(&mut delta_scale, delta.y, delta.z, delta.x, max.y, min.z, max.z, min.x, max.x, from.y, from.z, from.x);
    }

    if delta.z > 1.0E-7 {
        clip_point(&mut delta_scale, delta.z, delta.x, delta.y, min.z, min.x, max.x, min.y, max.y, from.z, from.x, from.y);
    } else if delta.z < -1.0E-7 {
        clip_point(&mut delta_scale, delta.z, delta.x, delta.y, max.z, min.x, max.x, min.y, max.y, from.z, from.x, from.y);
    }

    if delta_scale < 1.0 {
        return Some(from + delta * delta_scale);
    } else {
        return None;
    }
}

fn clip_point(delta_scale: &mut f64, delta1: f64, delta2: f64, delta3: f64, side1: f64, min2: f64, max2: f64, min3: f64, max3: f64, origin1: f64, origin2: f64, origin3: f64) {
    let scale = (side1 - origin1) / delta1;
    let hit2 = origin2 + scale * delta2;
    let hit3 = origin3 + scale * delta3;

    if 0.0 < scale && scale < *delta_scale && min2 - 1.0E-7 < hit2 && hit2 < max2 + 1.0E-7 && min3 - 1.0E-7 < hit3 && hit3 < max3 + 1.0E-7 {
        *delta_scale = scale;
    }
}

fn check_inside_block<P: PlayerExtension>(player: &Player<P>, blockpos: IVec3, velocity: &mut UncertainVelocity, reset_fall_distance: &mut bool, stuck_multiplier: &mut DVec3, position: DVec3, on_ground: bool, flying: bool) {
    let getter = KnownBlockGetter::new(player);
    let id = getter.get_block(blockpos.x, blockpos.y, blockpos.z).unwrap_or(0);
    if id == 0 {
        return;
    }
    let Ok(block_state) = BlockState::try_from(id) else {
        return;
    };
    match block_state {
        BlockState::BubbleColumn { drag } => {
            if flying {
                return;
            }

            let above = getter.get_block(blockpos.x, blockpos.y + 1, blockpos.z).unwrap_or(0);
            let above_attributes = BlockAttributes::from_block_state(above);
            if above_attributes.collision_shape.is_empty() && above_attributes.fluid_state == 0 {
                // onAboveBubbleCol
                if drag {
                    velocity.set_y((velocity.get_y() - 0.03).max(-0.9));
                } else {
                    velocity.set_y((velocity.get_y() + 0.1).min(1.8));
                }
            } else {
                // onInsideBubbleColumn
                if drag {
                    velocity.set_y((velocity.get_y() - 0.03).max(-0.3));
                } else {
                    velocity.set_y((velocity.get_y() + 0.06).min(0.7));
                }
                *reset_fall_distance = true;
            }
        },
        BlockState::HoneyBlock {  } => {
            if on_ground {
                return;
            }
            if position.y > blockpos.y as f64 + 0.9375 - 1.0E-7 {
                return;
            }
            let old_delta_y = velocity.get_y() / 0.98_f32 as f64 + 0.08;
            if old_delta_y >= -0.08 {
                return;
            }

            let offset_x = (blockpos.x as f64 + 0.5 - position.x).abs();
            let offset_z = (blockpos.z as f64 + 0.5 - position.z).abs();
            let threshold = 0.4375 + (0.6_f32 / 2.0_f32) as f64;
            if offset_x + 1.0E-7 > threshold || offset_z + 1.0E-7 > threshold {
                if old_delta_y < -0.13 {
                    let slowdown = -0.05 / old_delta_y;
                    *velocity *= DVec3::new(slowdown, 1.0, slowdown);
                }
                velocity.set_y((-0.05 - 0.08) * 0.98_f32 as f64);
                
                *reset_fall_distance = true;
            }
        },
        BlockState::PowderSnow {  } => {
            if blockpos == position.floor().as_ivec3() {
                *stuck_multiplier = DVec3::new(0.9_f32 as f64, 1.5, 0.9_f32 as f64);
            }
        },
        BlockState::SweetBerryBush { age: _ } => {
            *stuck_multiplier = DVec3::new(0.8_f32 as f64, 0.75, 0.8_f32 as f64);
        },
        BlockState::Cobweb { } => {
            if player.known_client_state.effects[MobEffect::Weaving].is_some() {
                *stuck_multiplier = DVec3::new(0.5, 0.25, 0.5);
            } else {
                *stuck_multiplier = DVec3::new(0.25, 0.05_f32 as f64, 0.25);
            }
        }
        _ => {}
    }
}

pub(crate) fn get_block_pos_below_that_affects_my_movement(supporting: Option<IVec3>, position: DVec3) -> IVec3 {
    get_on_pos(supporting, position, 0.500001_f32)
}

pub(crate) fn get_on_pos_legacy(supporting: Option<IVec3>, position: DVec3) -> IVec3 {
    get_on_pos(supporting, position, 0.2_f32)
}

fn get_on_pos(supporting: Option<IVec3>, position: DVec3, distance: f32) -> IVec3 {
    if let Some(supporting) = supporting {
        // Note: vanilla has some extra logic here for fences/walls/fence gates, but I don't think that's necessary
        return supporting;
    } else {
        let x = position.x.floor() as i32;
        let y = (position.y - distance as f64).floor() as i32;
        let z = position.z.floor() as i32;
        IVec3::new(x, y, z)
    }
}

fn check_supporting_block<W: WorldExtension>(getter: KnownBlockGetter<W>, aabb: AABB, on_ground_no_blocks: bool, new_position: DVec3, movement: DVec3) -> Option<IVec3> {
    let below = AABB::new(DVec3::new(aabb.min().x, aabb.min().y - 1.0E-6, aabb.min().z), DVec3::new(aabb.max().x, aabb.min().y, aabb.max().z));
    let supporting_block = find_supporting_block(getter, below, new_position);
    if supporting_block.is_some() || on_ground_no_blocks {
        return supporting_block;
    }

    let previous_below = below.translate(DVec3::new(-movement.x, 0.0, -movement.z));
    return find_supporting_block(getter, previous_below, new_position);
}

fn find_supporting_block<W: WorldExtension>(getter: KnownBlockGetter<W>, aabb: AABB, position: DVec3) -> Option<IVec3> {
    let min = aabb.min();
    let max = aabb.max();
    let broad_min = min.floor().as_ivec3() - 1;
    let broad_max = max.floor().as_ivec3() + 1;

    let mut closest_block = None;
    let mut closest_distance = f64::MAX;

    // This specific ordering is used because of the way vanilla's find_supporting_block breaks ties using Vec3i#compareTo
    // Most Y, then most Z, then most X
    for y in (broad_min.y..broad_max.y+1).rev() {
        for z in (broad_min.z..broad_max.z+1).rev() {
            for x in (broad_min.x..broad_max.x+1).rev() {
                let Some(block) = getter.get_block(x, y, z) else {
                    continue;
                };

                if block == 0 {
                    continue;
                }

                let distance = position.distance_squared(DVec3::new(x as f64 + 0.5, y as f64 + 0.5, z as f64 + 0.5));
                if distance >= closest_distance {
                    continue;
                }

                let attr = BlockAttributes::from_block_state(block);

                for shape in attr.collision_shape {
                    if (shape[0] + x as f64) >= max.x || (shape[1] + y as f64) >= max.y || (shape[2] + z as f64) >= max.z || (shape[3] + x as f64) <= min.x || (shape[4] + y as f64) <= min.y || (shape[5] + z as f64) <= min.z {
                        continue;
                    }

                    closest_block = Some(IVec3::new(x, y, z));
                    closest_distance = distance;
                    break;
                }
            }
        }    
    }

    closest_block
}

fn move_towards_closest_space<P: PlayerExtension>(player: &Player<P>, velocity: &mut UncertainVelocity, suffocation_cache: &mut HashMap<(i32, i32), bool>, offset_x: f64, offset_z: f64) {
    let x = (player.last_client_position.x + offset_x).floor() as i32;
    let z = (player.last_client_position.z + offset_z).floor() as i32;

    if !suffocates_at(player, suffocation_cache, x, z) {
        return;
    }

    let mut closest_distance = f64::MAX;
    let mut push = (0.0, 0.0);

    // West
    if !suffocates_at(player, suffocation_cache, x-1, z) {
        closest_distance = player.last_client_position.x + offset_x - x as f64;
        push = (-0.1, 0.0);
    }

    // East
    let east_distance = 1.0 - (player.last_client_position.x + offset_x - x as f64);
    if east_distance < closest_distance && !suffocates_at(player, suffocation_cache, x+1, z) {
        closest_distance = east_distance;
        push = (0.1, 0.0);
    }

    // North
    let north_distance = player.last_client_position.z + offset_z - z as f64;
    if north_distance < closest_distance && !suffocates_at(player, suffocation_cache, x, z-1)  {
        closest_distance = north_distance;
        push = (0.0, -0.1);
    }

    // South
    let south_distance = 1.0 - (player.last_client_position.z + offset_z - z as f64);
    if south_distance < closest_distance && !suffocates_at(player, suffocation_cache, x, z+1)  {
        push = (0.0, 0.1);
    }

    if push.0 != 0.0 {
        velocity.set_x(push.0);
    }
    if push.1 != 0.0 {
        velocity.set_z(push.1);
    }
}

fn suffocates_at<P: PlayerExtension>(player: &Player<P>, suffocation_cache: &mut HashMap<(i32, i32), bool>, x: i32, z: i32) -> bool {
    if let Some(value) = suffocation_cache.get(&(x, z)) {
        return *value;
    }

    let collision_aabb = player.create_collision_aabb_at(player.last_client_position);
    let aabb = AABB::new(
        DVec3::new(x as f64, collision_aabb.min().y, z  as f64),
        DVec3::new(x as f64 + 1.0, collision_aabb.max().y, z  as f64 + 1.0),
    );
    let aabb = aabb.deflate(1E-7).unwrap();
    let suffocates = !is_free(KnownBlockGetter::new(player), aabb, false, true);
    
    suffocation_cache.insert((x, z), suffocates);
    return suffocates;
}

fn get_fluid_at_eye<P: PlayerExtension>(player: &Player<P>) -> Fluid {
    let eye_offset = match player.known_client_state.pose {
        Pose::FallFlying => 0.4,
        Pose::Sleeping => 0.2,
        Pose::Swimming => 0.4,
        Pose::SpinAttack => 0.4,
        Pose::Crouching => 1.27,
        _ => 1.62,
    };
    let getter = KnownBlockGetter::new(player);
    let eye_position = player.last_client_position + DVec3::new(0.0, eye_offset, 0.0);
    let eye_block_position = eye_position.floor().as_ivec3();
    let (fluid_state, fluid) = get_fluid(getter, eye_block_position.x, eye_block_position.y, eye_block_position.z);
    if fluid == Fluid::Empty {
        return Fluid::Empty;
    }

    let (_, above_fluid) = get_fluid(getter, eye_block_position.x, eye_block_position.y+1, eye_block_position.z);
    if is_same_fluid(fluid, above_fluid) {
        return fluid;
    }

    let fluid_attributes = FluidAttributes::from_fluid_state(fluid_state);
    let fluid_y = (eye_block_position.y as f32 + fluid_attributes.own_height) as f64;
    if fluid_y > eye_position.y {
        return fluid;
    } else {
        return Fluid::Empty;
    }
    
}

fn move_relative(yaw: f32, left: f32, forwards: f32, speed: f32, velocity: &mut UncertainVelocity, player_delta: DVec3,
    is_using_item: bool, moving_slowly_speed: Option<f32>, debug_move_inputs: DVec3
) {
    if is_using_item {
        let min = velocity.get_min();
        let max = velocity.get_max();

        let movement = calculate_relative_movement(yaw, left, forwards, speed, true, moving_slowly_speed, DVec3::ZERO);
        let no_slow_movement = calculate_relative_movement(yaw, left, forwards, speed, false, moving_slowly_speed, DVec3::ZERO);

        let adjusted_min_x = min.x + movement.x - 1E-4;
        let adjusted_max_x = max.x + movement.x + 1E-4;
        let adjusted_min_z = min.z + movement.z - 1E-4;
        let adjusted_max_z = max.z + movement.z + 1E-4;

        let increased_velocity = player_delta.x.abs() >= adjusted_min_x.abs().max(adjusted_max_x.abs()) ||
            player_delta.z.abs() >= adjusted_min_z.abs().max(adjusted_max_z.abs());

        if increased_velocity {
            // Player delta is outside expected range, possibly stopped using the item using 1-9 hotkeys
            *velocity += no_slow_movement;
            velocity.ignored_item_slow = true;
        } else {
            *velocity += movement;

            // We allow the player to slow down their xz velocity without penalty
            velocity.expand_xz_uncertainty_to(min.x + no_slow_movement.x, min.z + no_slow_movement.z);
            velocity.expand_xz_uncertainty_to(max.x + no_slow_movement.x, max.z + no_slow_movement.z);
        }
    } else {
        *velocity += calculate_relative_movement(yaw, left, forwards, speed, false, moving_slowly_speed, debug_move_inputs);
    }
}

fn calculate_relative_movement(yaw: f32, mut left: f32, mut forwards: f32, speed: f32, is_using_item: bool, moving_slowly_speed: Option<f32>, debug_move_inputs: DVec3) -> DVec3 {
    let mut input = mojang_math::normalize_vec2(Vec2::new(left, forwards));
    
    // modifyInput
    input *= 0.98;

    if is_using_item {
        input *= 0.2;
    }
    
    if let Some(moving_slowly_speed) = moving_slowly_speed {
        input *= moving_slowly_speed;
    }

    // modifyInputSpeedForSquareMovement
    let length = input.length();
    if length > 0.0 {
        let normalized = input * (1.0 / length);
        let abs = input.abs();
        let ratio = if abs.y > abs.x {
            abs.x / abs.y
        } else {
            abs.y / abs.x
        };
        let distance_to_unit_square = (1.0 + ratio * ratio).sqrt();
        let scale = (length * distance_to_unit_square).min(1.0);
        left = normalized.x * scale;
        forwards = normalized.y * scale;
    }

    if crate::debug::DEBUG_MOVEMENT && debug_move_inputs != DVec3::ZERO {
        if left != debug_move_inputs.x as f32 {
            println!("xxa is wrong. Server: {:?}. Client: {:?}", left, debug_move_inputs.x as f32);
        }
        if forwards != debug_move_inputs.z as f32 {
            println!("zza is wrong. Server: {:?}. Client: {:?}", forwards, debug_move_inputs.z as f32);
        }
    }

    let raw_input_vector = DVec3::new(left as f64, 0.0, forwards as f64);
    let raw_input_vector_length_sq = raw_input_vector.length_squared();
    if raw_input_vector_length_sq >= 1.0E-7 {
        let mut input_vector = if raw_input_vector_length_sq > 1.0 {
            mojang_math::normalize(raw_input_vector)
        } else {
            raw_input_vector
        };
        input_vector *= speed as f64;

        let sin = mojang_math::sin(yaw * 0.017453292_f32);
        let cos = mojang_math::cos(yaw * 0.017453292_f32);

        let movement_x = input_vector.x * cos as f64 - input_vector.z * sin as f64;
        let movement_z = input_vector.z * cos as f64 + input_vector.x * sin as f64;
        DVec3::new(movement_x, 0.0, movement_z)
    } else {
        DVec3::ZERO
    }
}

fn finalize_movement<P: PlayerExtension>(player: &Player<P>, trust_client: bool, mut velocity: UncertainVelocity, mut stuck_multiplier: DVec3, is_flying: bool, client_position: DVec3, is_in_water: bool) -> MovementPrediction {
    let step_height = player.known_client_state.step_height as f32;
    let can_back_off_from_edge = !is_flying && velocity.get_y() <= 0.0 && player.shift_pressed && (trust_client || is_above_ground(player, step_height));

    let client_movement = client_position - player.last_client_position;
    let getter = KnownBlockGetter::new(player);

    if trust_client {
        velocity.expand_xz_uncertainty_to_zero();
    }

    let use_stuck_multiplier = stuck_multiplier.length_squared() > 1.0E-7;

    if use_stuck_multiplier {
        velocity *= stuck_multiplier;
    }

    let do_additional_check_collisions = resolve_velocity_uncertainty(&mut velocity, client_movement, player, can_back_off_from_edge);
    let do_additional_check_collision_x = do_additional_check_collisions.0;
    let do_additional_check_collision_z = do_additional_check_collisions.1;

    let mut changed_velocity = velocity.get_expected();

    if use_stuck_multiplier {
        stuck_multiplier = DVec3::ZERO;
        velocity.set_zero();
    }

    // maybe back off from edge
    if can_back_off_from_edge && !trust_client { // We check !trust_client because the back-off isn't necessary since we expand xz uncertainty to zero
        let mut dx = changed_velocity.x.abs();
        let mut dz = changed_velocity.z.abs();
        let sign_x = changed_velocity.x.signum();
        let sign_z = changed_velocity.z.signum();
        let min_dx = (client_movement.x * sign_x).max(0.0);
        let min_dz = (client_movement.z * sign_z).max(0.0);

        while dx > min_dx && can_fall_at_least(player, dx * sign_x, 0.0, step_height) {
            dx -= 0.05;

            if dx <= min_dx {
                dx = min_dx;
                break;
            }
        }

        while dz > min_dz && can_fall_at_least(player, 0.0, dz * sign_z, step_height) {
            dz -= 0.05;

            if dz <= min_dz {
                dz = min_dz;
                break;
            }
        }

        while dx != 0.0 && dz != 0.0 && can_fall_at_least(player, dx * sign_x, dz * sign_z, step_height) {
            if dx > min_dx {
                dx -= 0.05;
                if dx < min_dx {
                    dx = min_dx;
                }
            }

            if dz > min_dz {
                dz -= 0.05;

                if dz < min_dz {
                    dz = min_dz;
                }
            }

            if dx <= min_dx && dz <= min_dz {
                break;
            }
        }

        changed_velocity.x = dx * sign_x;
        changed_velocity.z = dz * sign_z;
    }

    let aabb = player.create_collision_aabb();

    let mut movement = changed_velocity;

    let mut y_collision_above_with_solid_entity = false;
    if trust_client {
        if player.last_client_on_ground && client_position.y <= player.position.y && client_position.y > player.position.y + movement.y {
            // Downwards Y velocity reduction (eg. standing on top of a solid entity)
            movement.y = client_position.y - player.position.y;
        } else if client_position.y >= player.position.y && client_position.y < player.position.y + movement.y - 1E-7 {
            // Upwards Y velocity reduction (eg. hitting head on bottom of solid entity)
            movement.y = client_position.y - player.position.y;
            y_collision_above_with_solid_entity = true;
        } else if (player.on_ground || changed_velocity.y < 0.0) && client_position.y >= player.position.y && client_position.y <= player.position.y + step_height as f64 && (movement.y - client_movement.y).abs() > 1E-4 {
            // Y velocity increase (eg. stepping onto a solid entity)
            movement.y = client_position.y - player.position.y;
            y_collision_above_with_solid_entity = changed_velocity.y >= 0.0;
        }
    }

    let (mut result, alternate_result) = lenient_shape_cast(player, aabb, movement, client_movement);

    let mut movement = result.delta;

    let mut collide_x = (movement.x - changed_velocity.x).abs() >= 1E-5;
    let mut collide_y = movement.y != changed_velocity.y;
    let mut collide_z = (movement.z - changed_velocity.z).abs() >= 1E-5;
    let mut on_ground = collide_y && changed_velocity.y < 0.0;

    if trust_client {
        on_ground = player.last_client_on_ground;
        if on_ground {
            collide_y = true;
        } else {
            collide_y = y_collision_above_with_solid_entity;
        }
    }

    // Handle stepping
    let allow_stepping = step_height > 0.0 && (on_ground || player.on_ground); 
    if allow_stepping && !trust_client { // No point handling stepping if already trusted
        let mut horizontal_collision = collide_x || collide_z;
        let mut movement_before_step = movement;

        if !horizontal_collision {
            if let Some(alternate_result) = alternate_result.as_ref() {
                horizontal_collision = alternate_result.delta.x != changed_velocity.x || alternate_result.delta.z != changed_velocity.z;
                movement_before_step = alternate_result.delta;
            }
        }

        if horizontal_collision {
            let mut snapped_aabb = aabb;

            let snap_to_ground = collide_y && changed_velocity.y < 0.0;
            if snap_to_ground {
                snapped_aabb = snapped_aabb.translate(DVec3::new(0.0, movement_before_step.y, 0.0));
            }
    
            let mut search_bounds = snapped_aabb.expand(DVec3::new(changed_velocity.x, step_height as f64, changed_velocity.z));
            if !snap_to_ground {
                search_bounds = search_bounds.expand(DVec3::new(0.0, -1E-5, 0.0));
            }
    
            let step_candidates = collect_step_candidates(getter, search_bounds,
            snapped_aabb.min().y, step_height as f32, movement_before_step.y as f32);
    
            for step_candidate in step_candidates {
                let step_delta = DVec3::new(changed_velocity.x, step_candidate as f64, changed_velocity.z);
                // Note: This should possibly use lenient_shape_cast, but doing that introduces it's own edge cases
                // The edge case that would be fixed by using lenient is probably really rare anyways
                let mut stepped_movement = vanilla_shape_cast(KnownBlockGetter::new(player), snapped_aabb, step_delta);
                stepped_movement.y -= aabb.min().y - snapped_aabb.min().y;
                if stepped_movement.xz().length_squared() > movement_before_step.xz().length_squared() {
                    if stepped_movement.distance_squared(client_movement) > movement.distance_squared(client_movement) {
                        // Don't perform step if stepped movement is further away from client movement than normal
                        break; 
                    }
    
                    if step_delta.z.abs() > step_delta.x.abs() {
                        result.aabb_after_x = snapped_aabb.translate(stepped_movement);
                        result.aabb_after_z = snapped_aabb.translate(DVec3::new(0.0, stepped_movement.y, stepped_movement.z));
                    } else {
                        result.aabb_after_x = snapped_aabb.translate(DVec3::new(stepped_movement.x, stepped_movement.y, 0.0));
                        result.aabb_after_z = snapped_aabb.translate(stepped_movement);
                    }

                    result.delta = stepped_movement;
                    movement = stepped_movement;
                    collide_x = (movement.x - changed_velocity.x).abs() >= 1E-5;
                    collide_y = movement.y != changed_velocity.y;
                    collide_z = (movement.z - changed_velocity.z).abs() >= 1E-5;
                    on_ground = collide_y && changed_velocity.y < 0.0;
                    break;
                }
            }
        }
    }

    if !trust_client { // This isn't necessary with trust_client, since the client will send horizontal collision & velocity can be arbitrarily reduced
        if !collide_x && do_additional_check_collision_x {
            let check_collide_x_at = result.aabb_after_x;
            if changed_velocity.x < 0.0 {
                collide_x = collide_axis(KnownBlockGetter::new(player), check_collide_x_at, 0, -1E-7, 1E-7) != -1E-7;
            } else if changed_velocity.x > 0.0 {
                collide_x = collide_axis(KnownBlockGetter::new(player), check_collide_x_at, 0, 1E-7, 1E-7) != 1E-7;
            } else {
                let inflate = DVec3::new(1E-7, -1E-7, -1E-7);
                collide_x = !is_free(getter, check_collide_x_at.inflate_by_vec(inflate).unwrap(), false, false);
            }
        }
        if !collide_z && do_additional_check_collision_z {
            let check_collide_z_at = result.aabb_after_z;
            
            if changed_velocity.z < 0.0 {
                collide_z = collide_axis(KnownBlockGetter::new(player), check_collide_z_at, 2, -1E-7, 1E-7) != -1E-7;
            } else if changed_velocity.z > 0.0 {
                collide_z = collide_axis(KnownBlockGetter::new(player), check_collide_z_at, 2, 1E-7, 1E-7) != 1E-7;
            } else {
                let inflate = DVec3::new(-1E-7, -1E-7, 1E-7);
                collide_z = !is_free(getter, check_collide_z_at.inflate_by_vec(inflate).unwrap(), false, false);
            }
        }

        // newY != known.y && y == known.y && xyz.distance(known.xyz) < 1 && client_on_ground && known.y_collision
        // setY(known.y), velocity.y = 0, onGround = true

        // newX != known.x && x == known.x && xyz.distance(known.xyz) < 1 && client_horizontal_collision && known.x_collision
        // setX(known.x), velocity.x = 0, onGround = true
    }

    let new_client_position = move_towards(player.last_client_position + movement, client_position, 0.25);

    // setOnGroundWithMovement
    let main_supporting_block = if on_ground {
        let aabb = player.create_collision_aabb_at(new_client_position);
        check_supporting_block(getter, aabb, player.on_ground && player.main_supporting_block.is_none(),
            new_client_position, new_client_position - player.last_client_position)
    } else {
        None
    };

    // checkFallDamage
    if !is_in_water && !is_flying {
        let _ = get_fluid_height_and_do_fluid_pushing(player, new_client_position, &mut velocity, 0.014, 0.0);
    }

    // Horizontal collision velocity update
    if collide_x {
        velocity.set_x(0.0);
    }
    if collide_z {
        velocity.set_z(0.0);
    }
    if trust_client && player.last_client_horizontal_collision {
        velocity.expand_xz_uncertainty_to_zero();
    }

    // updateEntityMovementAfterFallOn
    if collide_y {
        let velocity_y = velocity.get_y();
        if !player.shift_pressed { // Fast path: all custom updateEntityMovementAfterFallOn effects (slime & bed) don't apply if "moving carefully"
            let on_pos_legacy = get_on_pos_legacy(main_supporting_block, new_client_position);
            let on_block_state = getter.get_block(on_pos_legacy.x, on_pos_legacy.y, on_pos_legacy.z).unwrap_or(0);
            match graphite_mc_constants::block::state_to_block(on_block_state).unwrap_or(Block::Air) {
                Block::SlimeBlock => {
                    if velocity_y < 0.0 {
                        velocity.set_y(-velocity_y);
                    }
                },
                Block::WhiteBed | Block::OrangeBed | Block::MagentaBed | Block::LightBlueBed | Block::YellowBed | Block::LimeBed | Block::PinkBed | Block::GrayBed |
                Block::LightGrayBed | Block::CyanBed | Block::PurpleBed | Block::BlueBed | Block::BrownBed | Block::GreenBed | Block::RedBed | Block::BlackBed => {
                    if velocity_y < 0.0 {
                        velocity.set_y(-velocity_y * 0.66_f32 as f64);
                    }
                },
                _ => {
                    velocity.set_y(0.0);
                }
            }
        } else {
            velocity.set_y(0.0);
        }
    }

    // Block speed factor
    let is_fall_flying = (player.known_client_state.shared_flags & (1 << 7)) != 0;
    let speed_factor = if is_flying || is_fall_flying {
        1.0
    } else {
        let current_block_state = getter.get_block(new_client_position.x.floor() as i32, new_client_position.y.floor() as i32, new_client_position.z.floor() as i32).unwrap_or(0);
        let current_block_speed_factor = BlockAttributes::from_block_state(current_block_state).speed_factor;
        let block = graphite_mc_constants::block::state_to_block(current_block_state).unwrap_or(Block::Air);
        if block == Block::Water || block == Block::BubbleColumn || current_block_speed_factor != 1.0 {
            current_block_speed_factor
        } else {
            let below_pos = get_block_pos_below_that_affects_my_movement(main_supporting_block, new_client_position);
            let below_block_state = getter.get_block(below_pos.x, below_pos.y, below_pos.z).unwrap_or(0);
            let below_block_speed_factor = BlockAttributes::from_block_state(below_block_state).speed_factor;
            below_block_speed_factor
        }
    };

    velocity *= DVec3::new(speed_factor as f64, 1.0, speed_factor as f64);

    let movement_length_sq = movement.length_squared();
    let actual_movement = if movement_length_sq > 1.0E-7 || changed_velocity.length_squared() - movement_length_sq < 1.0E-7 {
        movement
    } else {
        DVec3::ZERO
    };

    let horizontal_collision = if trust_client {
        player.last_client_horizontal_collision
    } else {
        collide_x || collide_z
    };

    MovementPrediction {
        movement: actual_movement,
        velocity,
        on_ground,
        horizontal_collision,
        main_supporting_block,
        no_jump_delay: 0,
        stuck_multiplier,
        is_swimming: false,
        fluid_heights: FluidHeights::default(),
        reset_fall_distance: false
    }
}

struct LenientShapeCastResult {
    delta: DVec3,
    aabb_after_x: AABB,
    aabb_after_z: AABB,
}

fn lenient_shape_cast<P: PlayerExtension>(player: &Player<P>, mut aabb: AABB, mut delta: DVec3, client_movement: DVec3) -> (LenientShapeCastResult, Option<LenientShapeCastResult>) {
    if delta.length_squared() == 0.0 {
        return (LenientShapeCastResult {
            delta: DVec3::ZERO,
            aabb_after_x: aabb,
            aabb_after_z: aabb,
        }, None);
    }

    let getter = KnownBlockGetter::new(player);

    if delta.y != 0.0 {
        delta.y = collide_axis(getter, aabb, 1, delta.y, 1E-7);
        aabb = aabb.translate(DVec3::new(0.0, delta.y, 0.0));
    }

    let aabb_after_y = aabb;
    let delta_after_y = delta;
    let mut aabb_after_z = aabb;

    let prioritize_z = delta.z.abs() > delta.x.abs();
    if prioritize_z && delta.z != 0.0 {
        delta.z = collide_axis(getter, aabb, 2, delta.z, 1E-7);
        aabb = aabb.translate(DVec3::new(0.0, 0.0, delta.z));

        aabb_after_z = aabb;
    }

    if delta.x != 0.0 {
        delta.x = collide_axis(getter, aabb, 0, delta.x, 1E-7);
        aabb = aabb.translate(DVec3::new(delta.x, 0.0, 0.0));
    }
    let aabb_after_x = aabb;

    if !prioritize_z {
        if delta.z != 0.0 {
            delta.z = collide_axis(getter, aabb, 2, delta.z, 1E-7);
            aabb = aabb.translate(DVec3::new(0.0, 0.0, delta.z));
        }

        aabb_after_z = aabb;
    }

    // Need to account for the possibility that our X/Z order is wrong
    if delta.x != 0.0 && delta.z != 0.0 {
        let possible_horizontal_collision = delta.x != delta.x || delta.z != delta.z || player.last_client_horizontal_collision;
        let delta_error = delta.distance_squared(client_movement);
        if possible_horizontal_collision && delta_error > 0.01*0.01 {
            let mut alternate_aabb = aabb_after_y;
            let mut alternate_delta = delta_after_y;

            let mut alternate_after_z_aabb = aabb_after_y;

            // Perform collisions in the other order
            if !prioritize_z {
                alternate_delta.z = collide_axis(getter, alternate_aabb, 2, alternate_delta.z, 1E-7);
                alternate_aabb = alternate_aabb.translate(DVec3::new(0.0, 0.0, alternate_delta.z));
                alternate_after_z_aabb = alternate_aabb;
            }
    
            alternate_delta.x = collide_axis(getter, alternate_aabb, 0, alternate_delta.x, 1E-7);
            alternate_aabb = alternate_aabb.translate(DVec3::new(alternate_delta.x, 0.0, 0.0));
            let alternate_after_x_aabb = alternate_aabb;
    
            if prioritize_z {
                alternate_delta.z = collide_axis(getter, alternate_aabb, 2, alternate_delta.z, 1E-7);
                alternate_after_z_aabb = alternate_aabb;
            }

            if alternate_delta.distance_squared(client_movement) < delta_error {
                // Looks like the other X/Z order was the correct one, so use that instead
                return (LenientShapeCastResult {
                    delta: alternate_delta,
                    aabb_after_x: alternate_after_x_aabb,
                    aabb_after_z: alternate_after_z_aabb,
                }, Some(LenientShapeCastResult {
                    delta,
                    aabb_after_x,
                    aabb_after_z,
                }));
            } else {
                return (LenientShapeCastResult {
                    delta,
                    aabb_after_x,
                    aabb_after_z,
                }, Some(LenientShapeCastResult {
                    delta: alternate_delta,
                    aabb_after_x: alternate_after_x_aabb,
                    aabb_after_z: alternate_after_z_aabb,
                }));
            }
        } 
    }

    (LenientShapeCastResult {
        delta,
        aabb_after_x,
        aabb_after_z,
    }, None)
}

fn resolve_velocity_uncertainty<P: PlayerExtension>(velocity: &mut UncertainVelocity, client_movement: DVec3, player: &Player<P>, can_back_off_from_edge: bool) -> (bool, bool) {
    let mut do_additional_check_collision = [false, false, false];

    let mut min = velocity.get_min();
    let mut expected = velocity.get_expected();
    let mut max = velocity.get_max();
    
    for axis in [0, 2] {
        if min[axis] == max[axis] {
            do_additional_check_collision[axis] = false;
        } else {
            do_additional_check_collision[axis] = player.last_client_horizontal_collision;
    
            if client_movement == DVec3::ZERO {
                if max[axis] < -2E-4 {
                    expected[axis] = max[axis];
                } else if min[axis] > 2E-4 {
                    expected[axis] = min[axis];
                } else {
                    expected[axis] = expected[axis].clamp(min[axis].max(-2E-4), max[axis].min(2E-4));
                }
    
                if min[axis] < 0.0 && can_back_off_from_edge {
                    // Velocity may be hidden by sneaking on edge, don't limit
                } else {
                    min[axis] = min[axis].max(-2E-4 * 2.0);
                }
                if max[axis] > 0.0 && can_back_off_from_edge {
                    // Velocity may be hidden by sneaking on edge, don't limit
                } else {
                    max[axis] = max[axis].min(2E-4 * 2.0);
                }
            } else {
                if player.last_client_position[axis] + client_movement[axis] != player.last_client_position[axis] + expected[axis] {
                    expected[axis] = client_movement[axis].clamp(min[axis], max[axis]);
                }
    
                if min[axis] < 0.0 && can_back_off_from_edge {
                    // Velocity may be hidden by sneaking on edge, don't limit
                } else {
                    min[axis] = expected[axis];
                }
                if max[axis] > 0.0 && can_back_off_from_edge {
                    // Velocity may be hidden by sneaking on edge, don't limit
                } else {
                    max[axis] = expected[axis];
                }
            }
    
            let new_client_pos = player.last_client_position[axis] + client_movement[axis];
            min[axis] += new_client_pos.next_after(std::f64::NEG_INFINITY) - new_client_pos;
            max[axis] += new_client_pos.next_after(std::f64::INFINITY) - new_client_pos;
        }
    }

    velocity.set_with_min_max(expected, min, max);
    
    (do_additional_check_collision[0], do_additional_check_collision[2])
}

fn collect_step_candidates<W: WorldExtension>(getter: KnownBlockGetter<W>, within: AABB, min_y: f64, step_height: f32, skip_y: f32) -> Vec<f32> {
    let min = within.min();
    let max = within.max();
    let broad_min = within.min().floor().as_ivec3() - 1;
    let broad_max = within.max().floor().as_ivec3() + 1;

    let mut heights = Vec::new();

    for x in broad_min.x..broad_max.x+1 {
        for y in broad_min.y..broad_max.y+1 {
            for z in broad_min.z..broad_max.z+1 {
                let Some(block) = getter.get_block(x, y, z) else {
                    continue;
                };

                if block == 0 {
                    continue;
                }

                let attr = BlockAttributes::from_block_state(block);
                for shape in attr.collision_shape {
                    if (shape[0] + x as f64) >= max.x || (shape[2] + z as f64) >= max.z || (shape[3] + x as f64) <= min.x || (shape[5] + z as f64) <= min.z {
                        continue;
                    }

                    let delta = (shape[4] + y as f64 - min_y) as f32;
                    if delta >= 0.0 && delta <= step_height && delta != skip_y && !heights.contains(&delta) {
                        heights.push(delta);
                    }
                }
            }
        }    
    }

    heights.sort_by(f32::total_cmp);
    heights
}

fn is_on_climbable<W: WorldExtension>(getter: KnownBlockGetter<W>, position: DVec3) -> bool {
    let position = position.floor().as_ivec3();

    let Some(block) = getter.get_block(position.x, position.y, position.z) else {
        return false;
    };

    if block == 0 {
        return false;
    }

    let attr = BlockAttributes::from_block_state(block);
    if attr.has_flag(BlockFlag::Climbable) {
        return true;
    }

    // Special case with open trapdoor above a ladder
    let Ok(BlockClass::TrapDoor { facing: trapdoor_facing, half: _, open, powered: _, waterlogged: _ }) = BlockClass::try_from(block) else {
        return false;
    };

    if !open {
        return false;
    }

    let Some(below) = getter.get_block(position.x, position.y-1, position.z) else {
        return false;
    };
    let Ok(BlockClass::Ladder { facing: ladder_facing, waterlogged: _ }) = BlockClass::try_from(below) else {
        return false;
    };

    return ladder_facing == trapdoor_facing;
}

fn move_towards(from: DVec3, to: DVec3, amount: f64) -> DVec3 {
    let delta = to - from;

    let length_squared = delta.length_squared();
    if length_squared <= amount * amount {
        return to;
    }

    let length_recip = length_squared.sqrt().recip();

    if length_recip.is_finite() && length_recip > 0.0 {
        from + delta * length_recip * amount
    } else {
        to
    }
}

fn is_above_ground<P: PlayerExtension>(player: &Player<P>, threshold: f32) -> bool {
    player.on_ground || (player.fall_distance < threshold && !can_fall_at_least(player, 0.0, 0.0, threshold - player.fall_distance))
}

fn can_fall_at_least<P: PlayerExtension>(player: &Player<P>, offset_x: f64, offset_z: f64, fall: f32) -> bool {
    let aabb = player.create_collision_aabb_at(player.last_client_position);
    let aabb = AABB::new(
        DVec3::new(aabb.min().x + offset_x, aabb.min().y - fall as f64 - 9.999999747378752E-6, aabb.min().z + offset_z),
        DVec3::new(aabb.max().x + offset_x, aabb.min().y, aabb.max().z + offset_z)
    );
    is_free(KnownBlockGetter::new(player), aabb, false, false)
}

fn count_collidable_entities<W: WorldExtension>(world: &World<W>, aabb: AABB, exclude_uuid: u128) -> (u8, u8) {
    let mut solid_count: u8 = 0;
    let mut soft_count: u8 = 0;

    for chunk_x in ((aabb.min().x.floor() as i32 - 8) >> 4) ..= ((aabb.max().x.floor() as i32 + 8) >> 4) {
        for chunk_z in ((aabb.min().z.floor() as i32 - 8) >> 4) ..= ((aabb.max().z.floor() as i32 + 8) >> 4) {
            let Some(chunk) = world.get_chunk(chunk_x, chunk_z) else {
                continue;
            };

            for solid_aabb in &chunk.solid_entity_aabbs {
                if solid_aabb.intersects_aabb(aabb) {
                    solid_count = solid_count.saturating_add(1);
                }
            }
            for soft_aabb in &chunk.soft_entity_aabbs {
                if soft_aabb.intersects_aabb(aabb) {
                    soft_count = soft_count.saturating_add(1);
                }
            }
            for (_, player) in &chunk.players {
                let player = unsafe { player.get().as_ref() };
                let Some(player) = player else {
                    continue;
                };

                if player.get_uuid() == exclude_uuid {
                    continue;
                }

                if player.create_collision_aabb().intersects_aabb(aabb) {
                    soft_count = soft_count.saturating_add(1);
                }
            }
        }
    }

    (solid_count, soft_count)
}

fn is_free<W: WorldExtension>(getter: KnownBlockGetter<W>, aabb: AABB, check_fluids: bool, only_suffocating: bool) -> bool {
    let min = aabb.min();
    let max = aabb.max();
    let broad_min = min.floor().as_ivec3() - 1;
    let broad_max = max.floor().as_ivec3() + 1;
    let narrow_min = min.floor().as_ivec3();
    let narrow_max = max.ceil().as_ivec3();

    for x in broad_min.x..broad_max.x+1 {
        for y in broad_min.y..broad_max.y+1 {
            for z in broad_min.z..broad_max.z+1 {
                let Some(block) = getter.get_block(x, y, z) else {
                    continue;
                };

                if block == 0 {
                    continue;
                }

                let attr = BlockAttributes::from_block_state(block);

                // If check_fluids, return false if AABB intersects block with non-empty fluid
                if check_fluids && attr.fluid_state != 0 && x >= narrow_min.x && y >= narrow_min.y && z >= narrow_min.z &&
                        x < narrow_max.x && y < narrow_max.y && z < narrow_max.z {
                    return false;
                }

                if only_suffocating && !attr.has_flag(BlockFlag::Suffocating) {
                    continue;
                }

                for shape in attr.collision_shape {
                    if (shape[0] + x as f64) >= max.x || (shape[1] + y as f64) >= max.y || (shape[2] + z as f64) >= max.z || (shape[3] + x as f64) <= min.x || (shape[4] + y as f64) <= min.y || (shape[5] + z as f64) <= min.z {
                        continue;
                    }

                    // Collided with solid block
                    return false;
                }
            }
        }    
    }

    true
}

fn calculate_pose<P: PlayerExtension>(player: &mut Player<P>, forced_pose: ForcedPose) -> Option<Pose> {
    if !forced_pose.can_fit_when(Pose::Swimming) {
        return None;
    }

    let pose = if (player.known_client_state.shared_flags & (1 << 7)) != 0 {
        Pose::FallFlying
    } else if player.is_sleeping() {
        Pose::Sleeping
    } else if (player.known_client_state.shared_flags & (1 << 4)) != 0 {
        Pose::Swimming
    } else if (player.known_client_state.living_flags & (1 << 2)) != 0 {
        Pose::SpinAttack
    } else if (player.known_client_state.shared_flags & (1 << 1)) != 0 && !player.known_client_state.is_flying {
        Pose::Crouching
    } else {
        Pose::Standing
    };
    if forced_pose.can_fit_when(pose) {
        return Some(pose);
    } else if forced_pose.can_fit_when(Pose::Crouching) {
        return Some(Pose::Crouching);
    } else {
        return Some(Pose::Swimming);
    }
}

fn calculate_forced_pose<P: PlayerExtension>(player: &Player<P>) -> ForcedPose {
    let mut pose = Pose::Standing;
    let Some(mut bounding_box) = player.create_collision_aabb_for_pose_at(pose, player.last_client_position).deflate(1E-6) else {
        return ForcedPose::from_pose(pose);
    };

    let broad_phase_min = bounding_box.min().floor().as_ivec3() - 1;
    let broad_phase_max = bounding_box.max().floor().as_ivec3() + 1;

    let getter = KnownBlockGetter::new(player);
    for x in broad_phase_min.x..broad_phase_max.x+1 {
        for y in broad_phase_min.y..broad_phase_max.y+1 {
            for z in broad_phase_min.z..broad_phase_max.z+1 {
                let Some(block) = getter.get_block(x, y, z) else {
                    continue;
                };

                if block == 0 {
                    continue;
                }

                let attr = BlockAttributes::from_block_state(block);
                for aabb in attr.collision_shape {
                    let collision_aabb = AABB::new(
                        DVec3::new(x as f64 + aabb[0], y as f64 + aabb[1], z as f64 + aabb[2]),
                        DVec3::new(x as f64 + aabb[3], y as f64 + aabb[4], z as f64 + aabb[5])
                    );
                    loop {
                        let intersects = bounding_box.intersects_aabb(collision_aabb);

                        if !intersects {
                            break;
                        }

                        // Downgrade pose
                        pose = match pose {
                            Pose::Standing => Pose::Crouching,
                            Pose::Crouching => Pose::Swimming,
                            _ => return ForcedPose::None
                        };

                        let Some(new_bounding_box) = player.create_collision_aabb_for_pose_at(pose, player.last_client_position).deflate(1E-6) else {
                            return ForcedPose::from_pose(pose);
                        };
                        bounding_box = new_bounding_box;
                    }
                }
            }
        }
    }

    ForcedPose::from_pose(pose)
}