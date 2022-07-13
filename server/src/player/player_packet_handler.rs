use anyhow::bail;
use protocol::{
    play::client::{
        self, AcceptTeleportation, ClientInformation, CustomPayload, MovePlayerPos,
        MovePlayerPosRot, MovePlayerRot, PlayerAction,
    },
    types::Action,
};
use queues::IsQueue;

use super::{Player, PlayerService};

impl<P: PlayerService> client::PacketHandler for Player<P> {
    const DEBUG: bool = true;

    fn handle_move_player_pos_rot(&mut self, packet: MovePlayerPosRot) -> anyhow::Result<()> {
        if self.waiting_teleportation_id.size() > 0 {
            return Ok(());
        }

        self.position.coord.x = packet.x;
        self.position.coord.y = packet.y;
        self.position.coord.z = packet.z;
        self.position.rot.yaw = packet.yaw;
        self.position.rot.pitch = packet.pitch;

        // todo: check for moving too fast

        Ok(())
    }

    fn handle_move_player_pos(&mut self, packet: MovePlayerPos) -> anyhow::Result<()> {
        if self.waiting_teleportation_id.size() > 0 {
            return Ok(());
        }

        self.position.coord.x = packet.x;
        self.position.coord.y = packet.y;
        self.position.coord.z = packet.z;

        // todo: check for moving too fast

        Ok(())
    }

    fn handle_move_player_rot(&mut self, packet: MovePlayerRot) -> anyhow::Result<()> {
        if self.waiting_teleportation_id.size() > 0 {
            return Ok(());
        }

        self.position.rot.yaw = packet.yaw;
        self.position.rot.pitch = packet.pitch;

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
                    bail!("brand must have <128 bytes");
                }
                self.settings.set_brand(std::str::from_utf8(packet.data)?);
            }
            _ => {
                println!("unknown custom payload: {:?}", packet);
            }
        }
        Ok(())
    }

    fn handle_player_action(&mut self, packet: PlayerAction) -> anyhow::Result<()> {
        match packet.action {
            Action::StartDestroyBlock => todo!(),
            Action::AbortDestroyBlock => todo!(),
            Action::StopDestroyBlock => todo!(),
            Action::DropAllItems => todo!(),
            Action::DropItem => todo!(),
            Action::ReleaseUseItem => todo!(),
            Action::SwapItemWithOffHand => todo!(),
        }

        Ok(())
    }
}
