use anyhow::bail;
use command::types::ParseState;
use minecraft_constants::block::BlockProperties;
use protocol::{
    play::{client::{
        self, AcceptTeleportation, ClientInformation, CustomPayload, MovePlayerPos,
        MovePlayerPosRot, MovePlayerRot, PlayerHandAction, PlayerMoveAction, MovePlayerOnGround, UseItem, UseItemOn,
    }, server::{AnimateEntity, EntityAnimation}},
    types::{HandAction, MoveAction, Hand},
};
use queues::IsQueue;

use crate::{inventory::inventory_handler::{InventoryHandler, InventorySlot, ItemSlot}, gamemode::GameMode, player::interaction::Interaction};

use super::{Player, PlayerService};

impl<P: PlayerService> client::PacketHandler for Player<P> {
    const DEBUG: bool = false;

    fn handle_move_player_pos_rot(&mut self, packet: MovePlayerPosRot) -> anyhow::Result<()> {
        if self.waiting_teleportation_id.size() > 0 {
            return Ok(());
        }

        self.client_position.coord.x = packet.x as _;
        self.client_position.coord.y = packet.y as _;
        self.client_position.coord.z = packet.z as _;
        self.client_position.rot.yaw = packet.yaw;
        self.client_position.rot.pitch = packet.pitch;
        self.on_ground = packet.on_ground;

        Ok(())
    }

    fn handle_move_player_pos(&mut self, packet: MovePlayerPos) -> anyhow::Result<()> {
        if self.waiting_teleportation_id.size() > 0 {
            return Ok(());
        }

        self.client_position.coord.x = packet.x as _;
        self.client_position.coord.y = packet.y as _;
        self.client_position.coord.z = packet.z as _;
        self.on_ground = packet.on_ground;

        Ok(())
    }

    fn handle_move_player_rot(&mut self, packet: MovePlayerRot) -> anyhow::Result<()> {
        if self.waiting_teleportation_id.size() > 0 {
            return Ok(());
        }

        self.client_position.rot.yaw = packet.yaw;
        self.client_position.rot.pitch = packet.pitch;
        self.on_ground = packet.on_ground;

        Ok(())
    }

    fn handle_move_player_on_ground(&mut self, packet: MovePlayerOnGround) -> anyhow::Result<()> {
        if self.waiting_teleportation_id.size() > 0 {
            return Ok(());
        }

        self.on_ground = packet.on_ground;

        Ok(())
    }

    fn handle_accept_teleportation(&mut self, packet: AcceptTeleportation) -> anyhow::Result<()> {
        // todo: make sure this is working correctly

        if let Ok(teleport_id) = self.waiting_teleportation_id.peek() {
            if teleport_id == packet.id {
                // Pop the teleport ID from the queue
                self.waiting_teleportation_id.remove().unwrap();

                // Reset the timer, the player has confirmed the teleport
                self.teleport_id_timer = 0;
            } else {
                // Wrong teleport ID! But lets not kick the player just yet...
                // Start a timer, if they don't send the correct ID within 20 ticks,
                // they will be kicked then.
                self.teleport_id_timer = 1;
            }
        }
        Ok(())
    }

    fn handle_client_information(&mut self, packet: ClientInformation) -> anyhow::Result<()> {
        self.settings.update(packet);
        Ok(())
    }

    fn handle_custom_payload(&mut self, packet: CustomPayload) -> anyhow::Result<()> {
        match packet.channel {
            "minecraft:brand" => {
                if packet.data.len() > 128 {
                    bail!("too many bytes in brand payload");
                }
                self.settings.set_brand(std::str::from_utf8(packet.data)?);
            }
            _ => {
                println!("unknown custom payload: {:?}", packet);
            }
        }
        Ok(())
    }

    fn handle_player_hand_action(&mut self, packet: PlayerHandAction) -> anyhow::Result<()> {
        match packet.action {
            HandAction::StartDestroyBlock => {
                println!("start destroy block!");

                if let Some(interaction) = self.interaction_state.try_abort_break_or_use() {
                    self.fire_interaction(interaction);
                }

                let pos = packet.block_pos;
                
                if let Some(destroy_ticks) = self.get_world().get_required_destroy_ticks(
                        pos.x, pos.y as _, pos.z, self.get_break_speed_multiplier()) {
                    let instabreak = self.abilities.gamemode == GameMode::Creative || destroy_ticks <= 1.0;

                    if instabreak {
                        self.interaction_state.ignore_swing_ticks = 6;
                    } else {
                        self.interaction_state.start_breaking(packet.block_pos);
                        self.interaction_state.ignore_swing_ticks = 1;
                    }
    
                    // Left click block
                    self.fire_interaction(Interaction::LeftClickBlock {
                        position: packet.block_pos,
                        face: packet.direction,
                        instabreak
                    });
                }           

                self.ack_block_sequence(packet.sequence);
            }
            HandAction::AbortDestroyBlock => {
                println!("abort destroy block!");

                if let Some(interaction) = self.interaction_state.try_abort_break() {
                    self.fire_interaction(interaction);
                }

                self.ack_block_sequence(packet.sequence);
            },
            HandAction::StopDestroyBlock => {
                println!("stop destroy block!");
                self.finish_breaking_block(&packet);
                self.ack_block_sequence(packet.sequence);
            },
            HandAction::DropAllItems => {
                self.interaction_state.ignore_swing_ticks = 1;
            },
            HandAction::DropItem => {
                self.interaction_state.ignore_swing_ticks = 1;
            },
            HandAction::ReleaseUseItem => {
                println!("release use item!");

                if let Some(interaction) = self.interaction_state.try_abort_use(true) {
                    self.fire_interaction(interaction);
                }
            },
            HandAction::SwapItemWithOffHand => (),
        }

        Ok(())
    }

    fn handle_use_item(&mut self, packet: UseItem) -> anyhow::Result<()> {
        println!("use item!");

        if !self.interaction_state.processed_use_item {
            self.interaction_state.processed_use_item = true;

            if let Some(interaction) = self.interaction_state.try_abort_break_or_use() {
                self.fire_interaction(interaction);
            }

            // Fire RightClick on Air
            if !self.interaction_state.processed_use_item_on {
                self.interaction_state.ignore_swing_ticks = 1;
                self.fire_interaction(Interaction::RightClickAir);
            }

            // Check if item can be used, to start continuous usage
            let slot = self.inventory.get(InventorySlot::Hotbar(0))?;
            match slot {
                ItemSlot::Empty => (),
                ItemSlot::Filled(item) => {
                    if item.properties.use_duration > 0 {
                        self.interaction_state.start_using(item.properties.use_duration as _);
                    }
                }
            }
        }

        self.ack_block_sequence(packet.sequence);
        self.inventory.sync(InventorySlot::Hotbar(0))?; // todo: actual hotbar slot

        Ok(())
    }

    fn handle_use_item_on(&mut self, packet: UseItemOn) -> anyhow::Result<()> {
        if !self.interaction_state.processed_use_item_on {
            self.interaction_state.processed_use_item_on = true;

            if let Some(interaction) = self.interaction_state.try_abort_break_or_use() {
                self.fire_interaction(interaction);
            }

            self.interaction_state.ignore_swing_ticks = 1;
            
            let hit = packet.block_hit;
            if !(0.0..=1.0).contains(&hit.offset_x) ||
                !(0.0..=1.0).contains(&hit.offset_y) ||
                !(0.0..=1.0).contains(&hit.offset_z) {
                bail!("invalid hit offset");
            }

            self.fire_interaction(Interaction::RightClickBlock {
                position: hit.position,
                face: hit.direction,
                offset: (hit.offset_x, hit.offset_y, hit.offset_z)
            });
        }

        self.ack_block_sequence(packet.sequence);

        Ok(())
    }

    fn handle_player_move_action(&mut self, packet: PlayerMoveAction) -> anyhow::Result<()> {
        match packet.action {
            MoveAction::PressShiftKey => (),
            MoveAction::ReleaseShiftKey => (),
            MoveAction::StopSleeping => (),
            MoveAction::StartSprinting => (),
            MoveAction::StopSprinting => (),
            MoveAction::StartRidingJump => (),
            MoveAction::StopRidingJump => (),
            MoveAction::OpenInventory => (),
            MoveAction::StartFallFlying => (),
        }

        Ok(())
    }

    fn handle_keep_alive(&mut self, packet: client::KeepAlive) -> anyhow::Result<()> {
        if packet.id == self.current_keep_alive {
            self.current_keep_alive = 0;
        }

        Ok(())
    }

    fn handle_chat_command(&mut self, packet: client::ChatCommand) -> anyhow::Result<()> {
        // todo: finalize this functionality, add comments

        if let Some(dispatch) = &mut self.get_world_mut().get_universe().root_dispatch_node {
            let mut parse_state = ParseState::new(packet.command);
            parse_state.push_ref(self, parse_state.full_span);
            parse_state.push_arg(
                unsafe {
                    std::mem::transmute::<std::any::TypeId, u64>(std::any::Any::type_id(self))
                },
                parse_state.full_span,
            );
            let result = dispatch.dispatch_with(parse_state);

            self.send_message(format!("{:?}", result));
        }

        Ok(())
    }

    fn handle_set_carried_item(&mut self, packet: client::SetCarriedItem) -> anyhow::Result<()> {
        if packet.slot > 8 {
            bail!("invalid slot")
        }
        self.selected_hotbar_slot = packet.slot as u8;
        Ok(())
    }

    fn handle_swing(&mut self, packet: client::Swing) -> anyhow::Result<()> {
        // Get animation corresponding to hand
        let animation = if packet.hand == Hand::Main {
            EntityAnimation::SwingMainHand
        } else {
            EntityAnimation::SwingOffHand
        };

        // Write animation packet as viewable, excluding self
        self.write_viewable_packet(&AnimateEntity {
            id: self.entity_id.as_i32(),
            animation,
        }, true);

        // Use the swing to perform interactions
        if self.interaction_state.ignore_swing_ticks == 0 {
            if let Some(position) = self.interaction_state.breaking_block {
                if let Some((first, _)) = self.clip_block_position(position) {
                    self.interaction_state.breaking_block_timer = 5;

                    self.fire_interaction(Interaction::ContinuousBreak {
                        position,
                        break_time: self.interaction_state.break_time,
                        distance: first
                    });
                }

                return Ok(());
            }

            // Bug: https://bugs.mojang.com/browse/MC-255057

            // Currently we have to use swing to fire this interaction...
            // However, https://bugs.mojang.com/browse/MC-255058 would allow the following interaction
            // to be fired by a ServerboundMissPacket, which is a lot less error prone
            self.fire_interaction(Interaction::LeftClickAir);
        }

        Ok(())
    }

    fn handle_set_creative_mode_slot(&mut self, packet: client::SetCreativeModeSlot) -> anyhow::Result<()> {
        if self.abilities.gamemode == GameMode::Creative {
            self.inventory.creative_mode_set(packet.slot as usize, packet.item)?;
        }

        Ok(())
    }


}

impl<P: PlayerService> Player<P> {
    fn ack_block_sequence(&mut self, sequence: i32) {
        match self.ack_sequence_up_to {
            Some(old) => if old >= sequence { return },
            None => (),
        }
        self.ack_sequence_up_to = Some(sequence);
    }

    fn finish_breaking_block(&mut self, packet: &PlayerHandAction) {
        if let Some(position) = self.interaction_state.breaking_block {
            // Check if breaking location matches packet location 
            if position != packet.block_pos {   
                // Packet location was incorrect, abort the break
                let interaction = self.interaction_state.try_abort_break().expect("break must be active");
                self.fire_interaction(interaction);
            }

            // Make sure player is looking at block, get distance     
            if let Some((first, _)) = self.clip_block_position(position) {
                // Finish the block break
                let interaction = self.interaction_state.try_finish_break(first)
                    .expect("break must be active");
                self.fire_interaction(interaction);
            } else {
                // Player wasn't looking at correct block, abort break
                let interaction = self.interaction_state.try_abort_break().expect("break must be active");
                self.fire_interaction(interaction);
            }

            self.interaction_state.ignore_swing_ticks = 6;
        }
    }
}