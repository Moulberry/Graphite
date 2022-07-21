use binary::slice_serialization::SliceSerializable;
use net::{network_buffer::WriteBuffer, packet_helper};
use protocol::{play::server, IdentifiedPacket};

use crate::world::chunk::Chunk;

use super::position::{Coordinate, Rotation};

#[derive(Clone, Debug)]
pub struct Viewable {
    pub coord: Coordinate,
    pub(crate) index_in_chunk_entity_slab: usize,
    pub(crate) initialized: bool,
    pub(crate) last_chunk_x: i32,
    pub(crate) last_chunk_z: i32,
    pub(crate) buffer: *mut WriteBuffer,
    pub(crate) create_buffer: WriteBuffer,
    pub(crate) destroy_buffer: WriteBuffer,
}

// Don't worry about it
unsafe impl Send for Viewable {}
unsafe impl Sync for Viewable {}

impl Viewable {
    pub fn new(coord: Coordinate) -> Self {
        Self {
            index_in_chunk_entity_slab: 0,
            initialized: false,
            last_chunk_x: Chunk::to_chunk_coordinate(coord.x),
            last_chunk_z: Chunk::to_chunk_coordinate(coord.z),
            coord,
            buffer: std::ptr::null_mut(),
            create_buffer: WriteBuffer::with_min_capacity(64),
            destroy_buffer: WriteBuffer::with_min_capacity(64),
        }
    }

    // Update packets
    pub fn write_viewable_packet<'a, T>(&mut self, packet: &'a T) -> anyhow::Result<bool>
    where
        T: SliceSerializable<'a, T> + IdentifiedPacket<server::PacketId> + 'a,
    {
        if let Some(buffer) = unsafe { self.buffer.as_mut() } {
            packet_helper::write_packet(buffer, packet)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    // Create packets
    pub fn clear_create_packets(&mut self) {
        self.create_buffer.reset();
    }

    pub fn write_create_packet<'a, T>(&mut self, packet: &'a T) -> anyhow::Result<()>
    where
        T: SliceSerializable<'a, T> + IdentifiedPacket<server::PacketId> + 'a,
    {
        packet_helper::write_packet(&mut self.create_buffer, packet)
    }

    // Destroy packets
    pub fn clear_destroy_packets(&mut self) {
        self.destroy_buffer.reset();
    }

    pub fn write_destroy_packet<'a, T>(&mut self, packet: &'a T) -> anyhow::Result<()>
    where
        T: SliceSerializable<'a, T> + IdentifiedPacket<server::PacketId> + 'a,
    {
        packet_helper::write_packet(&mut self.destroy_buffer, packet)
    }
}

pub struct TestEntity {
    pub spawned: bool,
    pub entity_type: i32,
}

pub struct Spinalla {
    pub rotation: Rotation,
    pub reverse: bool,
}
