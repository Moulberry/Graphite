use std::fmt::Debug;

pub mod handshake;
pub mod status;
pub mod login;
pub mod play;

pub trait IdentifiedPacket<I> {
    fn get_packet_id(&self) -> I;
    fn get_packet_id_as_u8(&self) -> u8;
}

pub trait Packet<'a, I, T = Self> : Debug+IdentifiedPacket<I> {
    fn read(bytes: &'a [u8]) -> anyhow::Result<T>;
    fn get_write_size(&self) -> usize;
    unsafe fn write<'b>(&self, bytes: &'b mut [u8]) -> &'b mut [u8];
}

macro_rules! identify_packets {
    ( $enum_name:ident, $( $packet:ident $(<$life:lifetime>)? = $val:tt ),* ) => {
        #[derive(Debug, TryFromPrimitive)]
        #[repr(u8)]
        pub enum $enum_name {
            $( $packet = $val,)*
        }

        $(impl IdentifiedPacket<$enum_name> for $packet $(<$life>)? {
            fn get_packet_id(&self) -> $enum_name {
                $enum_name::$packet
            }
            fn get_packet_id_as_u8(&self) -> u8 {
                $enum_name::$packet as u8
            }
        })*
    }
}

pub(crate) use identify_packets;