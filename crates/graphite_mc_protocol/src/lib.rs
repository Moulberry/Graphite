use std::fmt::Debug;

pub mod handshake;
pub mod login;
pub mod configuration;
pub mod play;
pub mod status;
pub mod types;

pub trait IdentifiedPacket<I: Debug>: Debug {
    const ID: I;

    fn get_packet_id(&self) -> I;
    fn get_packet_id_as_u8(&self) -> u8;

    fn write_packet<'r, 'd: 'r>(&'r self, buffer: &mut PacketBuffer) where Self: SliceSerializable<'r, 'd, Self> {
        buffer.write_serializable(self.get_packet_id_as_u8(), self)
    }
}

macro_rules! identify_packets {
    { $enum_name:ident, $( $packet:ident $(<$life:lifetime>)? = $val:tt ),* } => {
        #[derive(Copy, Clone, Debug, TryFromPrimitive, Eq, PartialEq)]
        #[repr(u8)]
        pub enum $enum_name {
            $( $packet = $val,)*
        }

        $(impl IdentifiedPacket<$enum_name> for $packet $(<$life>)? {
            const ID: $enum_name = $enum_name::$packet;

            fn get_packet_id(&self) -> $enum_name {
                Self::ID
            }
            fn get_packet_id_as_u8(&self) -> u8 {
                Self::ID as u8
            }
        })*

        pub fn debug_print_packet(mut bytes: &[u8]) -> String {
            let packet_id_byte: u8 = graphite_binary::slice_serialization::Single::read(&mut bytes)
                .expect("packet must start with varint for the id");

            if let Ok(packet_id) = $enum_name::try_from(packet_id_byte) {
                match packet_id {
                    $(
                        $enum_name::$packet => {
                            let packet = $packet::read_fully(&mut bytes)
                                .expect(&format!("unable to read packet by id: 0x{:x}", packet_id_byte));
                            return format!("{:?}", packet);
                        }
                    )*
                }
            } else {
                panic!("unknown packet_id 0x{:x}", packet_id_byte);
            }
        }

        // pub fn debug_handle_packet<'a, T, F>(mut bytes: &'a [u8], func: F)
        // where
        //     F: FnOnce(&mut T),
        //     T: IdentifiedPacket<PacketId> + 'a
        // {
        //     let packet_id_byte: u8 = graphite_binary::slice_serialization::Single::read(&mut bytes)
        //         .expect("packet must start with varint for the id");

        //     if let Ok(packet_id) = $enum_name::try_from(packet_id_byte) {
        //         match packet_id {
        //             $(
        //                 $enum_name::$packet => {
        //                     let mut packet = $packet::read_fully(&mut bytes)
        //                         .expect(&format!("unable to read packet by id: 0x{:x}", packet_id_byte));
        //                     if packet.get_packet_id() != T::ID {
        //                         panic!("expected packet by id: 0x{:x}, got 0x{:x} instead", T::ID as u8, packet_id_byte);
        //                     }
        //                     (func)(unsafe { &mut *(&mut packet as *mut _ as *mut T) })
        //                 }
        //             )*
        //         }
        //     } else {
        //         panic!("unknown packet_id 0x{:x}", packet_id_byte);
        //     }
        // }

        pub trait PacketHandler<T: Default = ()> {
            const DEBUG: bool = false;

            paste::paste! {
                $(
                    fn [<handle_ $packet:snake>](&mut self, _: $packet) -> anyhow::Result<T> {
                        Ok(T::default())
                    }
                )*
            }

            fn parse_and_handle(&mut self, mut bytes: &[u8], skip_packet: impl FnOnce(&mut Self, $enum_name) -> bool) -> anyhow::Result<T> {
                let packet_id_byte: u8 = graphite_binary::slice_serialization::Single::read(&mut bytes)?;

                if let Ok(packet_id) = $enum_name::try_from(packet_id_byte) {
                    if (skip_packet)(self, packet_id) {
                        return Ok(T::default());
                    }

                    match packet_id {
                        $(
                            $enum_name::$packet => {
                                let mut bytes_wrapper = std::panic::AssertUnwindSafe(bytes);
                                let mut self_wrapper = std::panic::AssertUnwindSafe(self);
                                let unwind = std::panic::catch_unwind(move || {
                                    let packet_result = $packet::read_fully(&mut bytes_wrapper);

                                    match packet_result {
                                        Ok(packet) => {
                                            if Self::DEBUG {
                                                println!("<= {:?}", packet);
                                            }
                                            paste::paste! {
                                                Ok(self_wrapper.[<handle_ $packet:snake>](packet)?)
                                            }
                                        }
                                        Err(err) => {
                                            println!("Error while parsing packet: 0x{:x}", packet_id_byte);
                                            Err(err)
                                        }
                                    }
                                });
                                match unwind {
                                    Ok(result) => result,
                                    Err(_) => {
                                        println!("Panic while parsing or processing packet, disconnecting");
                                        anyhow::bail!("Panicked");
                                    }
                                }
                            }
                        )*
                    }
                } else {
                    if Self::DEBUG {
                        println!("<= Unknown packet 0x{:x}", packet_id_byte)
                    }
                    Ok(T::default())
                }
            }
        }
    }
}

use graphite_binary::slice_serialization::SliceSerializable;
use graphite_network::PacketBuffer;
pub(crate) use identify_packets;
