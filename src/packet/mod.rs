use self::handshake::ClientHandshake;

pub mod handshake;
pub mod status;
use std::{any::TypeId, fmt::Debug};

pub mod login;

enum ProtocolState {
    Handshake,
    Status,
    Login,
    Play
}

pub trait IdentifiedPacket<I> {
    fn get_packet_id() -> I;
}

pub trait Packet<'a, I, T = Self> : Debug+IdentifiedPacket<I> {
    fn read(bytes: &'a [u8]) -> anyhow::Result<T>;
    fn get_write_len_hint(&self) -> usize;
    fn write(&self, vec: &mut Vec<u8>); // todo: Vec<u8> could be `T: binary_writer::BinaryWritable + bytes::BufMut`
}

fn packet_id_of<'a, T: 'static + Packet<'a, T>>() -> u8 {
    if TypeId::of::<T>() == TypeId::of::<ClientHandshake>() {
        0
    } else {
        panic!("unknown packet type");
    }
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