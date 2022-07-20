use binary::slice_serialization::SliceSerializable;
use net::{network_buffer::WriteBuffer, packet_helper};
use protocol::{IdentifiedPacket, play::server};

use super::position::{Coordinate, Rotation};

#[derive(Clone, Debug)]
pub struct Viewable {
    pub coord: Coordinate,
    pub buffer: *mut WriteBuffer,
    pub create_buffer: WriteBuffer,
    pub destroy_buffer: WriteBuffer,
}

// Don't worry about it
unsafe impl Send for Viewable {}
unsafe impl Sync for Viewable {}

impl Viewable {
    // Update packets
    pub fn write_viewable_packet<'a, T>(&mut self, packet: &'a T) -> anyhow::Result<bool>
    where
        T: SliceSerializable<'a, T> + IdentifiedPacket<server::PacketId> + 'a
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
        T: SliceSerializable<'a, T> + IdentifiedPacket<server::PacketId> + 'a
    {
        packet_helper::write_packet(&mut self.create_buffer, packet)
    }

    // Destroy packets
    pub fn clear_destroy_packets(&mut self) {
        self.destroy_buffer.reset();
    }

    pub fn write_destroy_packet<'a, T>(&mut self, packet: &'a T) -> anyhow::Result<()>
    where
        T: SliceSerializable<'a, T> + IdentifiedPacket<server::PacketId> + 'a
    {
        packet_helper::write_packet(&mut self.destroy_buffer, packet)
    }
}

pub struct TestEntity {
    pub spawned: bool,
    pub entity_type: i32
}


pub struct Spinalla {
    pub rotation: Rotation
}