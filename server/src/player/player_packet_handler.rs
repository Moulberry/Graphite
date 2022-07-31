use anyhow::bail;
use command::types::ParseState;
use protocol::{
    play::{client::{
        self, AcceptTeleportation, ClientInformation, CustomPayload, MovePlayerPos,
        MovePlayerPosRot, MovePlayerRot, PlayerHandAction, PlayerMoveAction, MovePlayerOnGround,
    }, server::{AnimateEntity, EntityAnimation}},
    types::{HandAction, MoveAction, Hand},
};
use queues::IsQueue;

use crate::inventory::inventory_handler::InventoryHandler;

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
                let pos = packet.block_pos;

                // todo: move to function in world. remove magic 16s
                // todo: validate chunk_x/chunk_z is in-bounds
                let chunk_x = (pos.x / 16) as usize;
                let chunk_z = (pos.z / 16) as usize;

                let chunk = &mut self.get_world_mut().chunks[chunk_x][chunk_z];
                chunk.set_block(pos.x as _, pos.y as _, pos.z as _, 0);
            }
            HandAction::AbortDestroyBlock => (),
            HandAction::StopDestroyBlock => (),
            HandAction::DropAllItems => (),
            HandAction::DropItem => (),
            HandAction::ReleaseUseItem => (),
            HandAction::SwapItemWithOffHand => (),
        }

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

        Ok(())
    }

    fn handle_set_creative_mode_slot(&mut self, packet: client::SetCreativeModeSlot) -> anyhow::Result<()> {
        // todo: check if player is in creative
        self.inventory.creative_mode_set(packet.slot as usize, packet.item)?;

        Ok(())
    }
}
