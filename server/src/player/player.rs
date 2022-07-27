use std::mem::ManuallyDrop;

use anyhow::bail;
use binary::slice_serialization::SliceSerializable;
use net::{
    network_buffer::WriteBuffer,
    packet_helper::{self, PacketReadResult},
};
use protocol::{
    play::{client::PacketHandler, server},
    IdentifiedPacket,
};
use queues::Buffer;
use rand::RngCore;
use sticky::Unsticky;
use text_component::TextComponent;

use crate::{
    entity::position::{Position, Vec3f},
    universe::{EntityId, UniverseService},
    world::{ChunkViewPosition, World, WorldService},
};

use super::{
    player_connection::ConnectionReference, player_settings::PlayerSettings,
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

    // type InventoryHandlerType: InventoryHandler;
    // fn get_inventory_handler(player: &mut Player<Self>) -> &mut Self::InventoryHandlerType;
}

// graphite player

pub struct Player<P: PlayerService> {
    moved_into_proto: bool,
    pub service: ManuallyDrop<P>,
    connection: ManuallyDrop<ConnectionReference<P::UniverseServiceType>>,

    pub(crate) write_buffer: WriteBuffer,
    pub(crate) disconnected: bool,

    world: *mut World<P::WorldServiceType>,
    entity_id: EntityId,
    pub settings: PlayerSettings,

    last_position: Position,
    pub position: Position,
    pub(crate) client_position: Position,

    pub(crate) chunk_view_position: ChunkViewPosition,
    pub(crate) teleport_id_timer: u8,
    pub(crate) waiting_teleportation_id: Buffer<i32>,

    pub(crate) current_keep_alive: u64,
    keep_alive_timer: u8,
}

// graphite player impl

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
            moved_into_proto: false,
            service: ManuallyDrop::new(service),
            connection: ManuallyDrop::new(connection),

            write_buffer: WriteBuffer::new(),
            disconnected: false,

            world,
            entity_id,
            settings: PlayerSettings::new(),

            last_position: position,
            position,
            client_position: position,

            chunk_view_position: view_position,
            teleport_id_timer: 0,
            waiting_teleportation_id: Buffer::new(20),

            current_keep_alive: 0,
            keep_alive_timer: 0,
        }
    }

    pub fn get_world<'a, 'b>(&'a self) -> &'b World<P::WorldServiceType> {
        unsafe { self.world.as_ref().unwrap() }
    }

    pub fn get_world_mut<'a, 'b>(&'a mut self) -> &'b mut World<P::WorldServiceType> {
        unsafe { self.world.as_mut().unwrap() }
    }

    pub(crate) fn tick(&mut self) -> anyhow::Result<()> {
        if self.disconnected {
            bail!("player has been disconnected");
        }

        // Get spot packets
        let chunk_x = self.chunk_view_position.x as i32;
        let chunk_z = self.chunk_view_position.z as i32;
        let current_chunk = &self.get_world().chunks[chunk_x as usize][chunk_z as usize];
        self.write_buffer.copy_from(current_chunk.spot_buffer.get_written());

        // Get viewable packets
        let view_distance = P::WorldServiceType::ENTITY_VIEW_DISTANCE as i32;
        for x in (chunk_x - view_distance).max(0)
            ..(chunk_x + view_distance + 1).min(P::WorldServiceType::CHUNKS_X as _)
        {
            for z in (chunk_z - view_distance).max(0)
                ..(chunk_z + view_distance + 1).min(P::WorldServiceType::CHUNKS_Z as _)
            {
                let chunk = &self.get_world().chunks[x as usize][z as usize];

                self.write_buffer.copy_from(chunk.viewable_buffer.get_written());
            }
        }

        // Check teleport timer
        if self.teleport_id_timer > 0 {
            self.teleport_id_timer += 1;

            if self.teleport_id_timer >= 20 {
                bail!("player sent incorrect teleport id and failed to rectify within time limit");
            }
        }

        // Sending keep alive timer
        self.keep_alive_timer = self.keep_alive_timer.wrapping_add(1);
        if self.keep_alive_timer == 0 {
            if self.current_keep_alive != 0 {
                bail!("client hasn't responded with keep alive")
            }
            self.current_keep_alive = rand::thread_rng().next_u64();

            self.write_packet(&server::KeepAlive {
                id: self.current_keep_alive,
            });
        }

        // Update position
        if self.position != self.last_position {
            self.handle_movement(self.position, true)?;
        } else {
            self.handle_movement(self.client_position, false)?;
        }

        // Write packets from buffer
        let written_bytes = self.write_buffer.get_written();
        if !written_bytes.is_empty() {
            self.connection.write_bytes(written_bytes);
            self.write_buffer.reset();
        }
        self.write_buffer.tick_and_maybe_shrink();

        Ok(())
    }

    fn handle_movement(&mut self, to: Position, _inform_client: bool) -> anyhow::Result<()> {
        let distance_sq = to.distance_sq(self.last_position);
        let rotation_changed = self.client_position.rot.is_diff_u8(self.last_position.rot);
        let coord_changed = distance_sq > 0.0001;

        // todo: check for moving too fast
        // holdup: don't have server velocity

        if rotation_changed || coord_changed {
            self.chunk_view_position = self.get_world_mut().update_view_position(self, to)?;

            self.position = to;
            self.last_position = to;
        }

        Ok(())
    }

    pub fn send_message<T: Into<TextComponent>>(&mut self, message: T) {
        self.write_packet(&server::SystemChat {
            message: message.into().to_json(),
            overlay: false,
        })
    }

    pub fn disconnect(&mut self) {
        self.disconnected = true;
    }

    pub(crate) fn write_packet<'a, T>(&mut self, packet: &'a T)
    where
        T: SliceSerializable<'a, T> + IdentifiedPacket<server::PacketId> + 'a,
    {
        if packet_helper::write_packet(&mut self.write_buffer, packet).is_err() {
            // Packet was too big
            self.disconnect();
        }
    }

    pub(crate) fn handle_packets(player: *mut Player<P>) -> anyhow::Result<u32> {
        // Read all the bytes
        let mut bytes = unsafe { player.as_ref().unwrap() }.connection.read_bytes();

        // Get the player that received the packets
        let player: &mut Player<P> = unsafe { player.as_mut() }.unwrap();

        // Split, parse and handle all the received packets
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

        // Send contents of write buffer if FAST_PACKET_RESPONSE is enabled
        if P::FAST_PACKET_RESPONSE {
            let to_write = player.write_buffer.get_written();
            if !to_write.is_empty() {
                player.connection.write_bytes(to_write);
            }
            player.write_buffer.reset();
        }

        // Return remaining bytes
        Ok(bytes.len() as u32)
    }

    pub(crate) fn handle_disconnect(player: *mut Player<P>) {
        unsafe {
            let player: &mut Player<P> = &mut *player;
            player.connection.forget();
            player.disconnect();
        }
    }
}

impl<P: PlayerService> Drop for Player<P> {
    fn drop(&mut self) {
        if !self.moved_into_proto {
            unsafe {
                ManuallyDrop::drop(&mut self.connection);
                ManuallyDrop::drop(&mut self.service);
            }
        }

        // Safety: we are dropping the player
        unsafe {
            self.get_world_mut().decrease_player_count_in_chunk(self.chunk_view_position);
        }
    }
}

unsafe impl<P: PlayerService> Unsticky for Player<P> {
    type UnstuckType = (ProtoPlayer<P::UniverseServiceType>, P);

    fn update_pointer(&mut self, _: usize) {
        let ptr: *mut Player<P> = self;
        self.connection.update_player_pointer(ptr);
    }

    fn unstick(mut self) -> Self::UnstuckType {
        self.moved_into_proto = true; // Prevent calling drop on connection and service

        // Safety: `self.moved_into_proto = true` means that the following values
        // will not be dropped, so its safe to take them
        let connection = unsafe {
            ManuallyDrop::take(&mut self.connection)
        };
        let service = unsafe {
            ManuallyDrop::take(&mut self.service)
        };

        // Return the ProtoPlayer and Service as a tuple
        (
            ProtoPlayer::new(connection, self.entity_id.clone()),
            service,
        )
    }
}
