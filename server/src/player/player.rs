use std::{mem::ManuallyDrop, ops::Range};

use anyhow::bail;
use binary::slice_serialization::SliceSerializable;
use net::{
    network_buffer::WriteBuffer,
    packet_helper::{self, PacketReadResult},
};
use protocol::{
    play::{client::PacketHandler, server::{self, PlayerInfoAddPlayer, PlayerInfo, AddPlayer, RemoveEntities, TeleportEntity, RotateHead}},
    IdentifiedPacket, types::GameProfile,
};
use queues::Buffer;
use rand::RngCore;
use sticky::Unsticky;
use text_component::TextComponent;

use crate::{
    entity::position::{Position, Vec3f},
    universe::{EntityId, UniverseService},
    world::{ChunkViewPosition, World, WorldService, TickPhase},
};

use super::{
    player_connection::{AbstractConnectionReference}, player_settings::PlayerSettings,
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

#[allow(type_alias_bounds)] // Justification: used as a shortcut to avoid monsterous type
type ConnectionReferenceType<P: PlayerService> = <P::UniverseServiceType as UniverseService>::ConnectionReferenceType;

// graphite player
pub struct Player<P: PlayerService> {
    pub(crate) write_buffer: WriteBuffer,
    pub(crate) viewable_self_exclusion_write_buffer: WriteBuffer,
    pub(crate) disconnected: bool,

    world: *mut World<P::WorldServiceType>,
    pub profile: GameProfile,
    pub(crate) entity_id: EntityId,
    pub settings: PlayerSettings,

    last_position: Position,
    pub position: Position,
    pub(crate) client_position: Position,

    viewable_exclusion_range: Range<usize>,
    pub(crate) chunk_view_position: ChunkViewPosition,
    pub(crate) new_chunk_view_position: ChunkViewPosition,
    pub(crate) chunk_ref: usize,
    pub(crate) teleport_id_timer: u8,
    pub(crate) waiting_teleportation_id: Buffer<i32>,

    pub(crate) current_keep_alive: u64,
    keep_alive_timer: u8,

    moved_into_proto: bool,
    pub service: ManuallyDrop<P>,
    connection: ManuallyDrop<ConnectionReferenceType<P>>,
}

// graphite player impl

impl<P: PlayerService> Player<P> {
    pub(crate) fn new(
        service: P,
        world: &mut World<P::WorldServiceType>,
        profile: GameProfile,
        entity_id: EntityId,
        position: Position,
        view_position: ChunkViewPosition,
        connection: ConnectionReferenceType<P>,
    ) -> Self {
        Self {
            moved_into_proto: false,
            service: ManuallyDrop::new(service),
            connection: ManuallyDrop::new(connection),

            write_buffer: WriteBuffer::new(),
            viewable_self_exclusion_write_buffer: WriteBuffer::new(),
            disconnected: false,

            world,
            profile,
            entity_id,
            settings: PlayerSettings::new(),

            last_position: position,
            position,
            client_position: position,

            viewable_exclusion_range: 0..0,
            new_chunk_view_position: view_position,
            chunk_view_position: view_position,
            chunk_ref: usize::MAX,
            teleport_id_timer: 0,
            waiting_teleportation_id: Buffer::new(20),

            current_keep_alive: 0,
            keep_alive_timer: 0
        }
    }

    pub fn get_world<'a, 'b>(&'a self) -> &'b World<P::WorldServiceType> {
        unsafe { self.world.as_ref().unwrap() }
    }

    pub fn get_world_mut<'a, 'b>(&'a mut self) -> &'b mut World<P::WorldServiceType> {
        unsafe { self.world.as_mut().unwrap() }
    }

    pub(crate) fn tick(&mut self, tick_phase: TickPhase) -> anyhow::Result<()> {
        if self.disconnected {
            bail!("player has been disconnected");
        }

        if tick_phase == TickPhase::View {
            // Copy viewable packets
            let chunk_x = self.chunk_view_position.x as i32;
            let chunk_z = self.chunk_view_position.z as i32;
            let view_distance = P::WorldServiceType::ENTITY_VIEW_DISTANCE as i32;
            for x in (chunk_x - view_distance).max(0)
                ..(chunk_x + view_distance + 1).min(P::WorldServiceType::CHUNKS_X as _)
            {
                for z in (chunk_z - view_distance).max(0)
                    ..(chunk_z + view_distance + 1).min(P::WorldServiceType::CHUNKS_Z as _)
                {
                    let chunk = &self.get_world().chunks[x as usize][z as usize];

                    let bytes = chunk.viewable_buffer.get_written();

                    if x == chunk_x && z == chunk_z {
                        self.write_buffer.copy_from(&bytes[..self.viewable_exclusion_range.start]);
                        self.write_buffer.copy_from(&bytes[self.viewable_exclusion_range.end..]);
                        self.viewable_exclusion_range = 0..0;
                    } else {
                        self.write_buffer.copy_from(bytes);
                    }
                }
            }

            self.chunk_view_position = self.new_chunk_view_position;

            // Write packets from buffer
            if !self.write_buffer.is_empty() {
                // Write bytes into player connection
                self.connection.write_bytes(self.write_buffer.get_written());

                // Reset the write buffer
                self.write_buffer.reset();
            }
            self.write_buffer.tick_and_maybe_shrink();

            // Return early -- code after here is for TickPhase::Update
            return Ok(())
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

        // Write packets from viewable self-exclusion
        // These packets are seen by those in render distance of this player,
        // but *NOT* this player. This is used for eg. movement
        if !self.viewable_self_exclusion_write_buffer.is_empty() {
            let chunk = &mut self.get_world_mut().chunks[self.chunk_view_position.x as usize][self.chunk_view_position.z as usize];
            let write_to = &mut chunk.viewable_buffer;

            // Copy bytes into viewable buffer
            let start = write_to.len();
            write_to.copy_from(self.viewable_self_exclusion_write_buffer.get_written());
            let end = write_to.len();

            // Set exclusion range
            self.viewable_exclusion_range = start..end;

            // Reset the write buffer
            self.viewable_self_exclusion_write_buffer.reset();
        }
        self.viewable_self_exclusion_write_buffer.tick_and_maybe_shrink();

        Ok(())
    }

    fn handle_movement(&mut self, to: Position, _inform_client: bool) -> anyhow::Result<()> {
        let distance_sq = to.distance_sq(self.last_position);
        let rotation_changed = self.client_position.rot.is_diff_u8(self.last_position.rot);
        let coord_changed = distance_sq > 0.0001;

        // todo: check for moving too fast
        // holdup: don't have server velocity

        if coord_changed {
            // Teleport
            let teleport_packet = TeleportEntity {
                entity_id: self.entity_id.as_i32(),
                x: to.coord.x as _,
                y: to.coord.y as _,
                z: to.coord.z as _,
                yaw: to.rot.yaw,
                pitch: to.rot.pitch,
                on_ground: false,
            };
            self.write_viewable_packet(&teleport_packet, true);

            // Rotate head
            let rotate_head = RotateHead {
                entity_id: self.entity_id.as_i32(),
                head_yaw: to.rot.yaw,
            };
            self.write_viewable_packet(&rotate_head, true);

            self.get_world_mut().update_view_position(self, to)?;
        }

        self.position = to;
        self.last_position = to;
        self.client_position = to;

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

    pub fn write_packet<'a, T>(&mut self, packet: &'a T)
    where
        T: SliceSerializable<'a, T> + IdentifiedPacket<server::PacketId> + 'a,
    {
        if packet_helper::write_packet(&mut self.write_buffer, packet).is_err() {
            // Packet was too big
            self.disconnect();
        }
    }

    pub fn write_viewable_packet<'a, T>(&mut self, packet: &'a T, exclude_self: bool)
    where
        T: SliceSerializable<'a, T> + IdentifiedPacket<server::PacketId> + 'a,
    {
        let write_to = if exclude_self {
            &mut self.viewable_self_exclusion_write_buffer
        } else {
            let chunk = &mut self.get_world_mut().chunks[self.chunk_view_position.x as usize][self.chunk_view_position.z as usize];
            &mut chunk.viewable_buffer
        };

        if packet_helper::write_packet(write_to, packet).is_err() {
            // Packet was too big
            self.disconnect();
        }
    }

    pub(crate) fn write_destroy_packet(&mut self, write_buffer: &mut WriteBuffer) {
        // Remove Entity Packet
        let remove_entity_packet = RemoveEntities {
            entities: vec![self.entity_id.as_i32()],
        };
        net::packet_helper::write_packet(write_buffer, &remove_entity_packet).unwrap();

        // Remove Player Info
        let remove_info_packet = PlayerInfo::RemovePlayer {
            uuids: vec![self.profile.uuid]
        };
        net::packet_helper::write_packet(write_buffer, &remove_info_packet).unwrap();
    }

    pub(crate) fn write_create_packet(&mut self, write_buffer: &mut WriteBuffer) {
        let packet = PlayerInfo::AddPlayer {
            values: vec![
                PlayerInfoAddPlayer {
                    profile: self.profile.clone(),
                    gamemode: 1, // todo: gamemode
                    ping: 69, // todo: ping
                    display_name: None, 
                    signature_data: None
                }
            ]
        };
        net::packet_helper::write_packet(write_buffer, &packet).unwrap();

        let add_player_packet = AddPlayer {
            id: self.entity_id.as_i32(),
            uuid: self.profile.uuid,
            x: self.position.coord.x as _,
            y: self.position.coord.y as _,
            z: self.position.coord.z as _,
            yaw: 0.0,
            pitch: 0.0,
        };
        net::packet_helper::write_packet(write_buffer, &add_player_packet).unwrap();
    }

    pub(crate) fn write_packet_bytes(&mut self, bytes: &[u8]) {
        self.write_buffer.copy_from(bytes);
    }

    pub fn handle_packets(&mut self) -> anyhow::Result<u32> {
        // Read all the bytes
        // Safety: Nothing can modify the bytes that we have read
        let mut bytes = unsafe { &*(self.connection.read_bytes() as *const _) };

        // Split, parse and handle all the received packets
        loop {
            let packet_read_result = net::packet_helper::try_read_packet(&mut bytes)?;
            match packet_read_result {
                PacketReadResult::Complete(bytes) => {
                    self.parse_and_handle(bytes)?;
                }
                PacketReadResult::Partial => break,
                PacketReadResult::Empty => break,
            }
        }

        // Send contents of write buffer if FAST_PACKET_RESPONSE is enabled
        if P::FAST_PACKET_RESPONSE {
            let to_write = self.write_buffer.get_written();
            if !to_write.is_empty() {
                self.connection.write_bytes(to_write);
            }
            self.write_buffer.reset();
        }

        // Return remaining bytes
        Ok(bytes.len() as u32)
    }

    pub fn handle_disconnect(&mut self) {
        unsafe {
            self.connection.forget();
            self.disconnect();
        }
    }
}

impl<P: PlayerService> Drop for Player<P> {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            // Safety: we are dropping the player
            unsafe {
                self.get_world_mut().remove_player_from_chunk(self);
            }

            if !self.moved_into_proto {
                unsafe {
                    ManuallyDrop::drop(&mut self.connection);
                    ManuallyDrop::drop(&mut self.service);
                }
            }
        }
    }
}

unsafe impl<P: PlayerService> Unsticky for Player<P> {
    type UnstuckType = (ProtoPlayer<P::UniverseServiceType>, P);

    fn update_pointer(&mut self, _: usize) {
        let ptr: *mut Player<P> = self;
        self.connection.update_player_pointer(ptr);

        let world = self.get_world_mut();
        let chunk = &mut world.chunks[self.chunk_view_position.x as usize][self.chunk_view_position.z as usize];
        if self.chunk_ref == usize::MAX {
            chunk.add_new_player(self);
        } else {
            chunk.update_player_pointer(self);
        }
    }

    fn unstick(mut self) -> Self::UnstuckType {
        self.moved_into_proto = true; // Prevent calling drop on connection and service

        self.connection.clear_player_pointer();

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
            ProtoPlayer::new(connection, self.profile.clone(), self.entity_id.clone()),
            service,
        )
    }
}
