use std::fmt::Debug;

pub mod handshake;
pub mod status;
pub mod login;

pub trait IdentifiedPacket<I> {
    fn get_packet_id() -> I;
}

pub trait Packet<'a, I, T = Self> : Debug+IdentifiedPacket<I> {
    fn read(bytes: &'a [u8]) -> anyhow::Result<T>;
    fn get_write_size(&self) -> usize;
    unsafe fn write<'b>(&self, bytes: &'b mut [u8]) -> &'b mut [u8];
}

macro_rules! identify_packets {
    ( $( $packet:ident = $val:tt ),* ) => {
        use crate::packet::IdentifiedPacket;
        use derive_try_from_primitive::TryFromPrimitive;

        #[derive(Debug, TryFromPrimitive)]
        #[repr(u8)]
        pub enum PacketId {
            $( $packet = $val,)*
        }

        $(impl IdentifiedPacket<PacketId> for $packet<'_> {
            fn get_packet_id() -> PacketId {
                PacketId::$packet
            }
        })*
    }
}

pub(crate) use identify_packets;