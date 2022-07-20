use std::fmt::Debug;

pub mod handshake;
pub mod login;
pub mod play;
pub mod status;
pub mod types;

pub trait IdentifiedPacket<I: Debug> {
    fn get_packet_id(&self) -> I;
    fn get_packet_id_as_u8(&self) -> u8;
}

macro_rules! identify_packets {
    { $enum_name:ident, $( $packet:ident $(<$life:lifetime>)? = $val:tt ),* } => {
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

        pub trait PacketHandler {
            const DEBUG: bool = false;

            paste::paste! {
                $(
                    fn [<handle_ $packet:snake>](&mut self, _: $packet) -> anyhow::Result<()> {
                        Ok(())
                    }
                )*
            }

            fn parse_and_handle(&mut self, mut bytes: &[u8]) -> anyhow::Result<()> {
                let packet_id: u8 = binary::slice_serialization::VarInt::read(&mut bytes)?.try_into()?;

                if let Ok(packet_id) = $enum_name::try_from(packet_id) {
                    match packet_id {
                        $(
                            $enum_name::$packet => {
                                let packet = $packet::read_fully(&mut bytes)?;
                                if Self::DEBUG {
                                    println!("<= {:?}", packet);
                                }
                                paste::paste! {
                                    self.[<handle_ $packet:snake>](packet)
                                }
                            }
                        )*
                    }
                } else {
                    Ok(())
                    // anyhow::bail!("unknown packet_id 0x{:x}", packet_id)
                }
            }
        }
    }
}

pub(crate) use identify_packets;
