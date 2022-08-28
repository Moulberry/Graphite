use binary::slice_serialization::SliceSerializable;
use net::{network_buffer::WriteBuffer, packet_helper};
use protocol::{play::server, IdentifiedPacket};

pub struct PacketBuffer {
    pub(crate) write_buffer: WriteBuffer,
    pub(crate) viewable_self_exclusion_write_buffer: WriteBuffer,
    // pub(crate)
}

impl PacketBuffer {
    pub fn new() -> Self {
        Self {
            write_buffer: WriteBuffer::new(),
            viewable_self_exclusion_write_buffer: WriteBuffer::new(),
        }
    }

    pub fn write_raw_packets(&mut self, packet_bytes: &[u8]) {
        self.write_buffer.copy_from(packet_bytes);
    }

    pub fn write_packet<'a, T>(&mut self, packet: &'a T)
    where
        T: SliceSerializable<'a, T> + IdentifiedPacket<server::PacketId> + 'a,
    {
        let _ = packet_helper::write_packet(&mut self.write_buffer, packet);
    }

    pub fn write_viewable_packet<'a, T>(&mut self, packet: &'a T, exclude_self: bool)
    where
        T: SliceSerializable<'a, T> + IdentifiedPacket<server::PacketId> + 'a,
    {
        let write_to = if exclude_self {
            &mut self.viewable_self_exclusion_write_buffer
        } else {
            // let chunk = &mut self.get_world_mut().chunks[self.chunk_view_position.x as usize]
            //     [self.chunk_view_position.z as usize];
            // &mut chunk.entity_viewable_buffer
            todo!();
        };

        let _ = packet_helper::write_packet(write_to, packet);
    }
}
