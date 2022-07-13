use anyhow::bail;
use binary::slice_serialization::{self, SliceSerializable};
use net::{network_buffer::WriteBuffer, packet_helper::PacketReadResult};
use protocol::{play::client::{self, PacketHandler, MovePlayerPosRot, MovePlayerPos, MovePlayerRot, AcceptTeleportation, ClientInformation}, types::{ChatVisibility, ArmPosition}};
use queues::{Queue, Buffer, IsQueue};
use sticky::Unsticky;
use thiserror::Error;

use crate::{
    universe::{EntityId, UniverseService},
    world::{ChunkViewPosition, World, WorldService}, position::Position,
};

use super::{player_connection::ConnectionReference, proto_player::ProtoPlayer, player_settings::PlayerSettings};

// user defined player service trait

pub trait PlayerService
where
    Self: Sized,
{
    /// This will cause packets to be written immediately when packets are received
    /// If this is false, the server will instead wait for the tick
    ///
    /// Benefit: reduce latency by ~25ms (on average) for some interactions
    /// Drawback: two write operations could potentially strain the server
    const FAST_PACKET_RESPONSE: bool = true;

    type UniverseServiceType: UniverseService;
    type WorldServiceType: WorldService<UniverseServiceType = Self::UniverseServiceType>;

    // fn handle_event_xyz(event: Event) {}
}

/*pub trait PlayerEventHandle {
    fn handle_event_block_break(event: &mut BlockBreakEvent) {}
    fn handle_event_block_break(event: &mut BlockBreakEvent) {}
    fn handle_event_block_break(event: &mut BlockBreakEvent) {}
    fn handle_event_block_break(event: &mut BlockBreakEvent) {}
    fn handle_event_block_break(event: &mut BlockBreakEvent) {}
    fn handle_event_block_break(event: &mut BlockBreakEvent) {}
    fn handle_event_block_break(event: &mut BlockBreakEvent) {}
    fn handle_event_block_break(event: &mut BlockBreakEvent) {}
    fn handle_event_block_break(event: &mut BlockBreakEvent) {}
    fn handle_event_block_break(event: &mut BlockBreakEvent) {}
}

impl PlayerEventHandle for MyPlayerService {
    fn handle_event_block_break(event: &mut BlockBreakEvent) {
        // ...
    }
}

struct GamePlayer;

struct SpectatingPlayer;

impl BlockBreak for SpectatingPlayer {
    event.cancel();
}

struct BlockBreakEvent;

fn break_block() {
    if !can_break_block {
        return;
    }

    // player.handle_event_xyz(BlockBreakEvent);
    let vec: Vec<Box<dyn BlockBreakEvent>> = Default::default();
    player.handle_event(BlockBreakEvent);

    setblock to air
    remove the old block entity
    // ...
}*/

// graphite player

pub struct Player<P: PlayerService> {
    pub service: P,

    connection: ConnectionReference<P::UniverseServiceType>,
    write_buffer: WriteBuffer,

    world: *mut World<P::WorldServiceType>,
    entity_id: EntityId,
    settings: PlayerSettings,

    position: Position,
    view_position: ChunkViewPosition,
    teleport_id_timer: u8,
    waiting_teleportation_id: Buffer<i32>,
}

// graphite player impl

#[derive(Debug, Error)]
#[error("connection has been closed by the remote host")]
struct ConnectionClosedError;

impl<P: PlayerService> Player<P> {
    pub(crate) fn new(
        service: P,
        world: &mut World<P::WorldServiceType>,
        entity_id: EntityId,
        position: Position,
        view_position: ChunkViewPosition,
        connection: ConnectionReference<P::UniverseServiceType>,
    ) -> Self {
        Self {
            service,

            connection,
            write_buffer: WriteBuffer::new(),

            world,
            entity_id,
            settings: PlayerSettings::new(),

            position,
            view_position,
            teleport_id_timer: 0,
            waiting_teleportation_id: Buffer::new(20),
        }
    }

    pub(crate) fn tick(&mut self) -> anyhow::Result<()> {
        self.write_buffer.reset();
        self.write_buffer.tick_and_maybe_shrink();

        println!("player has position: {:?}", self.position);

        if self.teleport_id_timer > 0 {
            self.teleport_id_timer += 1;

            if self.teleport_id_timer == 20 {
                bail!("player sent incorrect teleport id and failed to rectify within time limit");
            }
        }

        Ok(())
    }

    pub(crate) fn handle_packets(player: *mut Player<P>) -> anyhow::Result<u32> {
        let mut bytes = unsafe { player.as_ref().unwrap() }.connection.read_bytes();

        let player: &mut Player<P> = unsafe { player.as_mut() }.unwrap();

        loop {
            let packet_read_result = net::packet_helper::try_read_packet(&mut bytes)?;
            match packet_read_result {
                PacketReadResult::Complete(bytes) => {
                    player.parse_and_handle(bytes)?;
                }
                PacketReadResult::Partial => break,
                PacketReadResult::Empty => break,
            }
        }

        let remaining_bytes = bytes.len() as u32;

        if P::FAST_PACKET_RESPONSE {
            let to_write = player.write_buffer.get_written();
            if !to_write.is_empty() {
                player.connection.write_bytes(to_write);
            }
            player.write_buffer.reset();
        }

        Ok(remaining_bytes)
    }

    pub(crate) fn handle_disconnect(player: *mut Player<P>) {
        unsafe {
            let player: &mut Player<P> = player.as_mut().unwrap();
            player.connection.forget();
        }

        todo!();
        // todo: remove self from PlayerList containing this
        // todo: notify world/universe of disconnect
    }
}

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
}

unsafe impl<P: PlayerService> Unsticky for Player<P> {
    type UnstuckType = (ProtoPlayer<P::UniverseServiceType>, P);

    fn update_pointer(&mut self, _: usize) {
        // todo: use index to be able to remove self from sticky
        let ptr: *mut Player<P> = self;
        self.connection.update_player_pointer(ptr);
    }

    fn unstick(self) -> Self::UnstuckType {
        (
            ProtoPlayer::new(self.connection, self.entity_id),
            self.service,
        )
    }
}
