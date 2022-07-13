use anyhow::bail;
use net::{network_buffer::WriteBuffer, packet_helper::PacketReadResult};
use protocol::play::client::PacketHandler;
use queues::Buffer;
use sticky::Unsticky;
use thiserror::Error;

use crate::{
    position::Position,
    universe::{EntityId, UniverseService},
    world::{ChunkViewPosition, World, WorldService},
};

use super::{
    player_connection::ConnectionReference, player_settings::PlayerSettings,
    proto_player::ProtoPlayer,
};

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
}

// graphite player

pub struct Player<P: PlayerService> {
    pub service: P,

    connection: ConnectionReference<P::UniverseServiceType>,
    write_buffer: WriteBuffer,

    world: *mut World<P::WorldServiceType>,
    entity_id: EntityId,
    pub settings: PlayerSettings,

    pub position: Position,
    pub(crate) view_position: ChunkViewPosition,
    pub(crate) teleport_id_timer: u8,
    pub(crate) waiting_teleportation_id: Buffer<i32>,
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

        // Check teleport timer
        if self.teleport_id_timer > 0 {
            self.teleport_id_timer += 1;

            if self.teleport_id_timer >= 20 {
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

        // todo: remove self from PlayerList containing this
        // todo: notify world/universe of disconnect
        todo!();
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
