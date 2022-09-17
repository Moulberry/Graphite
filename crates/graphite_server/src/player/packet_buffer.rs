use graphite_binary::slice_serialization::SliceSerializable;
use graphite_net::{network_buffer::WriteBuffer, packet_helper};
use graphite_mc_protocol::{play::server, IdentifiedPacket};

pub struct PacketBuffer {
    pub(crate) write_buffer: WriteBuffer,
    pub(crate) viewable_self_exclusion_write_buffer: WriteBuffer,
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
        let _ = packet_helper::try_write_packet(&mut self.write_buffer, packet);
    }

    pub fn write_self_excluded_viewable_packet<'a, T>(&mut self, packet: &'a T)
    where
        T: SliceSerializable<'a, T> + IdentifiedPacket<server::PacketId> + 'a,
    {
        let write_to = &mut self.viewable_self_exclusion_write_buffer;
        let _ = packet_helper::try_write_packet(write_to, packet);
    }
}
